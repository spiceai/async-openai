//! Aggregated logging for requests that are retried because they were rate limited.
//!
//! A saturated quota throttles every request in flight, so logging each one at
//! `WARN` drowns out the rest of the log. Instead the first throttled request per
//! model is reported at `WARN`, every later one at `DEBUG`, and a per-model
//! summary is emitted at `WARN` once per [`AGGREGATE_WINDOW`].

use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
    time::{Duration, Instant},
};

/// How long throttled requests are counted for before a summary is emitted.
const AGGREGATE_WINDOW: Duration = Duration::from_secs(60);

/// Number of models counted separately. Models past this share a single bucket so
/// that unexpected messages cannot grow the table without end.
const MAX_TRACKED_MODELS: usize = 32;

/// Bucket for throttled requests whose model could not be determined.
const UNKNOWN_MODEL: &str = "unknown model";

/// Longest model name given a bucket of its own.
const MAX_MODEL_LEN: usize = 64;

/// Records a throttled request and logs it, keeping the per-request detail out of
/// `WARN` once a model's first throttled request has been reported.
pub(crate) fn log_throttled(message: &str) {
    log_throttled_at(tracker(), message, Instant::now());
}

/// Adds the backoff a retry is about to sleep for to its model's running total.
pub(crate) fn record_backoff(message: &str, backoff: Duration) {
    record_backoff_on(tracker(), message, backoff);
}

/// Process-wide throttling state.
fn tracker() -> &'static Tracker {
    static TRACKER: OnceLock<Tracker> = OnceLock::new();
    TRACKER.get_or_init(Tracker::default)
}

fn log_throttled_at(tracker: &Tracker, message: &str, now: Instant) {
    let model = model_from_message(message).unwrap_or(UNKNOWN_MODEL);
    let report = tracker.record(model, now);

    if report.first {
        tracing::warn!("Rate limited: {message}");
    } else {
        tracing::debug!("Rate limited: {message}");
    }

    if let Some(summary) = report.summary {
        tracing::warn!(
            "Rate limited by {model}: {} requests throttled in the last {}s, total backoff {:.1}s",
            summary.throttled,
            summary.window.as_secs(),
            summary.backoff.as_secs_f64(),
        );
    }
}

/// Messages that are not rate-limit messages are ignored, so retries of other
/// transient errors do not contribute.
fn record_backoff_on(tracker: &Tracker, message: &str, backoff: Duration) {
    if let Some(model) = model_from_message(message) {
        tracker.add_backoff(model, backoff);
    }
}

/// Extracts the model from a rate-limit message, which reads
/// `Rate limit reached for <model> in organization <org> on <quota>: ...`.
///
/// Returns `None` for anything that does not match, which keeps the organisation
/// id out of the summary and confines unrecognised messages to one bucket.
fn model_from_message(message: &str) -> Option<&str> {
    let rest = message.strip_prefix("Rate limit reached for ")?;
    let model = &rest[..rest.find(" in organization ")?];
    (!model.is_empty() && model.len() <= MAX_MODEL_LEN).then_some(model)
}

/// Counts throttled requests per model.
#[derive(Debug, Default)]
struct Tracker {
    models: Mutex<HashMap<String, ModelState>>,
}

impl Tracker {
    fn record(&self, model: &str, now: Instant) -> Report {
        let mut models = self.lock();

        // Models past the limit share one bucket, to bound the table.
        let key = if models.len() < MAX_TRACKED_MODELS || models.contains_key(model) {
            model
        } else {
            UNKNOWN_MODEL
        };

        match models.entry(key.to_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(ModelState::opening(now));
                Report::first()
            }
            Entry::Occupied(mut entry) => {
                let state = entry.get_mut();

                // A whole window without a throttled request ends the episode, so
                // the next one is reported afresh.
                if now.duration_since(state.last) >= AGGREGATE_WINDOW {
                    *state = ModelState::opening(now);
                    return Report::first();
                }

                state.last = now;
                state.throttled += 1;

                let window = now.duration_since(state.start);
                if window < AGGREGATE_WINDOW {
                    return Report::later(None);
                }

                let summary = Summary {
                    throttled: state.throttled,
                    window,
                    backoff: state.backoff,
                };

                // This request belongs to the window it closed, so the next window
                // opens empty.
                state.start = now;
                state.throttled = 0;
                state.backoff = Duration::ZERO;

                Report::later(Some(summary))
            }
        }
    }

    fn add_backoff(&self, model: &str, backoff: Duration) {
        if let Some(state) = self.lock().get_mut(model) {
            state.backoff = state.backoff.saturating_add(backoff);
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, ModelState>> {
        self.models.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Throttled requests counted for one model since its current window opened.
#[derive(Debug)]
struct ModelState {
    start: Instant,
    last: Instant,
    throttled: u64,
    backoff: Duration,
}

impl ModelState {
    /// Opens a window already holding the throttled request being recorded.
    fn opening(now: Instant) -> Self {
        Self {
            start: now,
            last: now,
            throttled: 1,
            backoff: Duration::ZERO,
        }
    }
}

/// What to log for a single throttled request.
#[derive(Debug, PartialEq, Eq)]
struct Report {
    /// Whether this is the first throttled request of an episode, which is
    /// reported at `WARN` rather than `DEBUG`.
    first: bool,
    /// Totals for a window this request closed.
    summary: Option<Summary>,
}

impl Report {
    fn first() -> Self {
        Self {
            first: true,
            summary: None,
        }
    }

    fn later(summary: Option<Summary>) -> Self {
        Self {
            first: false,
            summary,
        }
    }
}

/// Totals for one model over a closed window.
#[derive(Debug, PartialEq, Eq)]
struct Summary {
    throttled: u64,
    window: Duration,
    backoff: Duration,
}

#[cfg(test)]
mod tests {
    use std::{
        fmt::Debug,
        sync::{Arc, Mutex},
    };

    use tracing::{
        field::{Field, Visit},
        span, Event, Level, Metadata, Subscriber,
    };

    use super::*;

    /// A rate-limit message of the shape OpenAI returns, with the organisation id
    /// redacted.
    const TPM_MESSAGE: &str = "Rate limit reached for text-embedding-3-small in organization \
                               org-redacted on tokens per min (TPM): Limit 10000000, Used \
                               10000000, Requested 71609. Please try again in 429ms.";

    #[test]
    fn model_is_read_from_a_rate_limit_message() {
        assert_eq!(
            model_from_message(TPM_MESSAGE),
            Some("text-embedding-3-small")
        );
    }

    #[test]
    fn other_messages_have_no_model() {
        assert_eq!(model_from_message("You exceeded your current quota."), None);
        assert_eq!(model_from_message("Rate limit reached for gpt-4"), None);
        assert_eq!(
            model_from_message("Rate limit reached for  in organization org-redacted on TPM"),
            None
        );
    }

    #[test]
    fn a_burst_within_one_window_reports_only_the_first_request() {
        let tracker = Tracker::default();
        let start = Instant::now();

        let reports: Vec<Report> = (0..500)
            .map(|i| tracker.record("gpt-4", start + Duration::from_millis(i * 100)))
            .collect();

        assert!(reports[0].first);
        assert_eq!(reports.iter().filter(|report| report.first).count(), 1);
        assert!(reports.iter().all(|report| report.summary.is_none()));
    }

    #[test]
    fn a_window_closing_summarises_the_requests_it_held() {
        let tracker = Tracker::default();
        let start = Instant::now();

        // One request a second, so the one at 60s closes the window.
        let reports: Vec<Report> = (0..=60)
            .map(|i| tracker.record("gpt-4", start + Duration::from_secs(i)))
            .collect();

        let summaries: Vec<&Summary> = reports
            .iter()
            .filter_map(|report| report.summary.as_ref())
            .collect();

        assert_eq!(reports.iter().filter(|report| report.first).count(), 1);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].throttled, 61);
        assert_eq!(summaries[0].window, AGGREGATE_WINDOW);
    }

    #[test]
    fn backoff_accumulates_into_the_open_window_only() {
        let tracker = Tracker::default();
        let start = Instant::now();

        tracker.record("gpt-4", start);
        tracker.add_backoff("gpt-4", Duration::from_millis(400));
        tracker.record("gpt-4", start + Duration::from_secs(30));
        tracker.add_backoff("gpt-4", Duration::from_millis(600));

        let report = tracker.record("gpt-4", start + AGGREGATE_WINDOW);
        let summary = report.summary.expect("the window should have closed");
        assert_eq!(summary.throttled, 3);
        assert_eq!(summary.backoff, Duration::from_secs(1));

        // The next window starts empty rather than carrying the total forward.
        tracker.record("gpt-4", start + AGGREGATE_WINDOW + Duration::from_secs(1));
        let report = tracker.record("gpt-4", start + AGGREGATE_WINDOW * 2);
        let summary = report
            .summary
            .expect("the second window should have closed");
        assert_eq!(summary.throttled, 2);
        assert_eq!(summary.backoff, Duration::ZERO);
    }

    #[test]
    fn each_model_is_counted_separately() {
        let tracker = Tracker::default();
        let start = Instant::now();

        assert!(tracker.record("gpt-4", start).first);
        assert!(tracker.record("text-embedding-3-small", start).first);
        assert!(
            !tracker
                .record("gpt-4", start + Duration::from_secs(1))
                .first
        );
    }

    #[test]
    fn a_quiet_window_starts_a_new_episode() {
        let tracker = Tracker::default();
        let start = Instant::now();

        tracker.record("gpt-4", start);
        assert!(
            !tracker
                .record("gpt-4", start + Duration::from_secs(1))
                .first
        );
        assert!(
            tracker
                .record("gpt-4", start + Duration::from_secs(1) + AGGREGATE_WINDOW)
                .first
        );
    }

    #[test]
    fn the_table_stays_bounded() {
        let tracker = Tracker::default();
        let start = Instant::now();

        for i in 0..10_000 {
            tracker.record(&format!("model-{i}"), start);
        }

        assert!(tracker.lock().len() <= MAX_TRACKED_MODELS + 1);
    }

    #[test]
    fn a_flood_of_throttled_requests_logs_one_warning_and_a_summary() {
        let events = Captured::default();
        let tracker = Tracker::default();
        let start = Instant::now();

        tracing::subscriber::with_default(events.clone(), || {
            // 500 throttled requests spread evenly over the window, the last of
            // which closes it.
            for i in 0..500_u32 {
                log_throttled_at(&tracker, TPM_MESSAGE, start + AGGREGATE_WINDOW * i / 499);
            }
        });

        let warnings = events.at(Level::WARN);
        assert_eq!(
            warnings.len(),
            2,
            "expected one first-occurrence warning and one summary, got {warnings:#?}"
        );
        assert!(warnings[0].starts_with("Rate limited: Rate limit reached for"));
        assert_eq!(
            warnings[1],
            "Rate limited by text-embedding-3-small: 500 requests throttled in the last 60s, \
             total backoff 0.0s"
        );
        assert_eq!(events.at(Level::DEBUG).len(), 499);
    }

    #[test]
    fn a_summary_reports_the_backoff_spent_waiting() {
        let events = Captured::default();
        let tracker = Tracker::default();
        let start = Instant::now();

        tracing::subscriber::with_default(events.clone(), || {
            log_throttled_at(&tracker, TPM_MESSAGE, start);
            record_backoff_on(&tracker, TPM_MESSAGE, Duration::from_millis(1100));
            log_throttled_at(&tracker, TPM_MESSAGE, start + Duration::from_secs(30));
            record_backoff_on(&tracker, TPM_MESSAGE, Duration::from_millis(400));
            log_throttled_at(&tracker, TPM_MESSAGE, start + AGGREGATE_WINDOW);
        });

        assert_eq!(
            events.at(Level::WARN)[1],
            "Rate limited by text-embedding-3-small: 3 requests throttled in the last 60s, \
             total backoff 1.5s"
        );
    }

    #[test]
    fn backoff_from_other_transient_errors_is_not_counted() {
        let tracker = Tracker::default();
        tracker.record("text-embedding-3-small", Instant::now());
        record_backoff_on(&tracker, "The server had an error.", Duration::from_secs(5));

        assert_eq!(
            tracker.lock()["text-embedding-3-small"].backoff,
            Duration::ZERO
        );
    }

    /// The organisation id belongs to the per-request detail, not the summary.
    #[test]
    fn the_summary_carries_no_organisation_id() {
        let events = Captured::default();
        let tracker = Tracker::default();
        let start = Instant::now();

        tracing::subscriber::with_default(events.clone(), || {
            log_throttled_at(&tracker, TPM_MESSAGE, start);
            log_throttled_at(&tracker, TPM_MESSAGE, start + Duration::from_secs(30));
            log_throttled_at(&tracker, TPM_MESSAGE, start + AGGREGATE_WINDOW);
        });

        assert!(!events.at(Level::WARN)[1].contains("org-"));
    }

    /// Collects the level and message of every event emitted while it is the
    /// active subscriber.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<(Level, String)>>>);

    impl Captured {
        fn at(&self, level: Level) -> Vec<String> {
            self.0
                .lock()
                .expect("the lock is only held by assertions that do not panic")
                .iter()
                .filter(|(emitted, _)| *emitted == level)
                .map(|(_, message)| message.clone())
                .collect()
        }
    }

    impl Subscriber for Captured {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
            Some(tracing::level_filters::LevelFilter::TRACE)
        }

        fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

        fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut message = String::new();
            event.record(&mut MessageVisitor(&mut message));
            self.0
                .lock()
                .expect("the lock is only held by assertions that do not panic")
                .push((*event.metadata().level(), message));
        }

        fn enter(&self, _span: &span::Id) {}

        fn exit(&self, _span: &span::Id) {}
    }

    /// Reads the `message` field of an event into a string.
    struct MessageVisitor<'a>(&'a mut String);

    impl Visit for MessageVisitor<'_> {
        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "message" {
                self.0.push_str(value);
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
            if field.name() == "message" {
                self.0.push_str(&format!("{value:?}"));
            }
        }
    }
}
