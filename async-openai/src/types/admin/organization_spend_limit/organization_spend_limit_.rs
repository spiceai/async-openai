use serde::{Deserialize, Serialize};

use super::{SpendLimitCurrency, SpendLimitEnforcement, SpendLimitInterval};

/// Confirmation payload returned after deleting an organization hard spend limit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrganizationSpendLimitDeletedResource {
    /// The object type, which is always `organization.spend_limit.deleted`.
    pub object: String,
    /// Whether the hard spend limit was deleted.
    pub deleted: bool,
}

/// Represents a hard spend limit configured at the organization level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrganizationSpendLimitResource {
    /// The object type, which is always `organization.spend_limit`.
    pub object: String,
    /// The hard spend limit amount, in cents.
    pub threshold_amount: i64,
    /// The currency for the threshold amount. Currently, only `USD` is supported.
    pub currency: SpendLimitCurrency,
    /// The time interval for evaluating spend against the threshold. Currently, only `month` is supported.
    pub interval: SpendLimitInterval,
    /// The current enforcement state of the hard spend limit.
    pub enforcement: SpendLimitEnforcement,
}

/// Parameters for the hard spend limit you want to create or replace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateOrganizationSpendLimitBody {
    /// The hard spend limit amount, in cents.
    pub threshold_amount: i64,
    /// The currency for the threshold amount. Currently, only `USD` is supported.
    pub currency: UpdateOrganizationSpendLimitBodyCurrency,
    /// The time interval for evaluating spend against the threshold. Currently, only `month` is supported.
    pub interval: UpdateOrganizationSpendLimitBodyInterval,
}

/// The currency for the threshold amount. Currently, only `USD` is supported.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdateOrganizationSpendLimitBodyCurrency {
    #[serde(rename = "USD")]
    USD,
}

/// The time interval for evaluating spend against the threshold. Currently, only `month` is supported.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateOrganizationSpendLimitBodyInterval {
    Month,
}

#[cfg(test)]
mod tests {
    use super::UpdateOrganizationSpendLimitBody;
    use serde_json::json;

    #[test]
    fn a_request_takes_only_the_currency_and_interval_the_api_accepts() {
        let body = json!({"threshold_amount": 100, "currency": "USD", "interval": "month"});
        let parsed: UpdateOrganizationSpendLimitBody =
            serde_json::from_value(body.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), body);
        for (currency, interval) in [("EUR", "month"), ("USD", "week")] {
            let unsupported =
                json!({"threshold_amount": 100, "currency": currency, "interval": interval});
            serde_json::from_value::<UpdateOrganizationSpendLimitBody>(unsupported.clone())
                .expect_err(&format!("{unsupported} is refused"));
        }
    }
}
