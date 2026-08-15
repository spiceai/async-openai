//! Sweep real Codex request bodies (captured from `codex ... wire_api=responses`
//! pointed at a Spice gateway) against `CreateResponse`.
//!
//! Two checks per fixture:
//!  1. Hard-fail: deserialize into `CreateResponse`; on failure report the exact
//!     serde path (which `input` item / field the untagged enum could not match).
//!  2. Silent mutation: deserialize -> reserialize and diff against the original
//!     JSON. A gateway that forwards the reserialized body must not drop or
//!     change any field the client sent.

#![cfg(feature = "responses")]

use async_openai::types::responses::{CreateResponse, Item};
use serde_json::Value;

/// Load every `*.json` under `tests/codex_fixtures/` as `(name, bytes)`.
fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/codex_fixtures");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read codex_fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            out.push((name, std::fs::read(&path).expect("read fixture")));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!out.is_empty(), "no fixtures found in {dir}");
    out
}

/// Report every JSON path present in `orig` that is missing or changed in `round`.
fn diff(path: &str, orig: &Value, round: &Value, out: &mut Vec<String>) {
    match (orig, round) {
        (Value::Object(o), Value::Object(r)) => {
            for (k, ov) in o {
                let child = format!("{path}/{k}");
                match r.get(k) {
                    Some(rv) => diff(&child, ov, rv, out),
                    None => out.push(format!("DROPPED  {child} = {}", truncate(ov))),
                }
            }
        }
        (Value::Array(o), Value::Array(r)) => {
            if o.len() != r.len() {
                out.push(format!(
                    "LEN      {path} original={} roundtrip={}",
                    o.len(),
                    r.len()
                ));
            }
            for (i, ov) in o.iter().enumerate() {
                if let Some(rv) = r.get(i) {
                    diff(&format!("{path}[{i}]"), ov, rv, out);
                }
            }
        }
        (a, b) if a != b => {
            out.push(format!(
                "CHANGED  {path} original={} roundtrip={}",
                truncate(a),
                truncate(b)
            ));
        }
        _ => {}
    }
}

fn truncate(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 80 {
        format!("{}…", &s[..80])
    } else {
        s
    }
}

#[test]
fn codex_bodies_deserialize_and_roundtrip_without_loss() {
    let mut failures = Vec::new();

    for (name, bytes) in &fixtures() {
        let original: Value = serde_json::from_slice(bytes).expect("fixture is valid JSON");

        // 1. Hard-fail path.
        let de = &mut serde_json::Deserializer::from_slice(bytes);
        match serde_path_to_error::deserialize::<_, CreateResponse>(de) {
            Ok(typed) => {
                // 2. Silent mutation.
                let round = serde_json::to_value(&typed).expect("reserialize typed body");
                let mut d = Vec::new();
                diff("", &original, &round, &mut d);
                if !d.is_empty() {
                    failures.push(format!(
                        "{name}: deserialized OK but {} field(s) mutated on roundtrip:\n  {}",
                        d.len(),
                        d.join("\n  ")
                    ));
                }
            }
            Err(err) => {
                // serde_path_to_error cannot descend into the untagged `InputParam`,
                // so bisect: `Item` is `type`-tagged, so per-item errors carry a
                // precise field path. Also roundtrip-diff each item that does type.
                let mut detail = vec![format!("HARD FAIL at `{}`: {err}", err.path())];
                if let Some(items) = original.get("input").and_then(Value::as_array) {
                    for (i, item) in items.iter().enumerate() {
                        let ty = item
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("<no type>");
                        let item_str = item.to_string();
                        let de = &mut serde_json::Deserializer::from_str(&item_str);
                        match serde_path_to_error::deserialize::<_, Item>(de) {
                            Ok(typed) => {
                                let round =
                                    serde_json::to_value(&typed).expect("reserialize item");
                                let mut d = Vec::new();
                                diff(&format!("input[{i}]({ty})"), item, &round, &mut d);
                                for line in d {
                                    detail.push(format!("  {line}"));
                                }
                            }
                            Err(e) => detail.push(format!(
                                "  input[{i}]({ty}) FAILS Item at `{}`: {e}",
                                e.path()
                            )),
                        }
                    }
                }
                failures.push(format!("{name}: {}", detail.join("\n")));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Codex gateway sweep found {} issue(s):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
