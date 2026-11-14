use nutype::nutype;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use url::Url;

use crate::people::Name;

use super::EventsConfig;
use super::api::{ApiError, REQUEST_TIMEOUT, Refusal, accepted, endpoint};

const ENQUEUE: [&str; 2] = ["v2", "enqueue"];
const SOURCE: &str = "Zulip";

/// ```
/// use wokesy::pagerduty::Summary;
///
/// assert_eq!(Summary::try_new("  lichess.org is down ").unwrap().as_str(), "lichess.org is down");
/// assert!(Summary::try_new("  ").is_err());
/// assert!(Summary::try_new("é".repeat(1024)).is_ok());
/// assert!(Summary::try_new("é".repeat(1025)).is_err());
/// ```
#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 1024),
    derive(Debug, Clone, PartialEq, Eq, Deref, Display)
)]
pub struct Summary(String);

#[derive(Clone, Debug)]
pub struct Events {
    http: reqwest::Client,
    url: Url,
    routing_key: SecretString,
}

#[derive(Serialize)]
struct Alert<'a> {
    routing_key: &'a str,
    event_action: Action,
    payload: Payload<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Trigger,
}

#[derive(Serialize)]
struct Payload<'a> {
    summary: &'a str,
    source: &'static str,
    severity: Severity,
    custom_details: Details<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Critical,
}

#[derive(Serialize)]
struct Details<'a> {
    paged_by: &'a str,
}

impl Events {
    #[must_use]
    pub fn new(http: reqwest::Client, config: &EventsConfig) -> Self {
        Self {
            http,
            url: config.url.clone(),
            routing_key: config.routing_key.clone(),
        }
    }

    pub async fn trigger(&self, summary: &Summary, paged_by: &Name) -> Result<(), ApiError> {
        let alert = Alert {
            routing_key: self.routing_key.expose_secret(),
            event_action: Action::Trigger,
            payload: Payload {
                summary,
                source: SOURCE,
                severity: Severity::Critical,
                custom_details: Details { paged_by },
            },
        };
        let request = self
            .http
            .post(endpoint(&self.url, &ENQUEUE))
            .json(&alert)
            .timeout(REQUEST_TIMEOUT);
        accepted::<Refusal>(request.send().await?).await.map(drop)
    }
}

#[cfg(test)]
mod tests;
