mod abilities;
mod custom_shifts;
mod escalation_policies;
mod oncalls;
mod schedules;
mod users;

use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use time::{OffsetDateTime, UtcOffset};
use url::Url;

use super::Config;

pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const LONGEST_WAIT_FOR_A_CHANGE: Duration = Duration::from_secs(10);
const VERSION_2: &str = "application/vnd.pagerduty+json;version=2";
const PAGE_SIZE: usize = 100;
const SHIFT_BASED_SCHEDULE: u32 = 3005;
const NO_REASON: &str = "no reason given";
const NO_ID: &str = "unknown";

#[derive(Clone, Debug)]
pub struct Api {
    http: reqwest::Client,
    base_url: Url,
    token: SecretString,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("pagerduty request failed")]
    Request(#[from] reqwest::Error),
    #[error("pagerduty answered {status} with a body of an unexpected shape")]
    Unreadable {
        status: StatusCode,
        #[source]
        cause: serde_json::Error,
    },
    #[error("pagerduty rejected the request ({status}): {reason}")]
    Rejected { status: StatusCode, reason: String },
    #[error("pagerduty rejected the request ({status}): {reason}")]
    ShiftBasedSchedule { status: StatusCode, reason: String },
}

#[derive(Deserialize)]
pub(super) struct Refusal {
    message: String,
    code: Option<Code>,
    #[serde(default)]
    errors: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Code {
    Number(u32),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Refused {
    Wrapped { error: Refusal },
    Bare(Refusal),
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(flatten)]
    entries: T,
    more: bool,
}

#[derive(Deserialize)]
struct Reference<I> {
    id: I,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Entry<T> {
    Read(T),
    Unreadable(serde_json::Value),
}

impl<T> Entry<T> {
    fn read_or_else(self, skip: impl FnOnce(&serde_json::Value)) -> Option<T> {
        match self {
            Self::Read(read) => Some(read),
            Self::Unreadable(entry) => {
                skip(&entry);
                None
            }
        }
    }
}

fn id_of(entry: &serde_json::Value) -> &str {
    entry
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(NO_ID)
}

fn id_of_its<'a>(entry: &'a serde_json::Value, reference: &str) -> &'a str {
    entry.get(reference).map_or(NO_ID, id_of)
}

impl Refusal {
    fn rejecting(self, status: StatusCode) -> ApiError {
        let shift_based = status == StatusCode::BAD_REQUEST
            && matches!(self.code, Some(Code::Number(SHIFT_BASED_SCHEDULE)));
        let reason = self.reason();
        if shift_based {
            ApiError::ShiftBasedSchedule { status, reason }
        } else {
            ApiError::Rejected { status, reason }
        }
    }

    fn reason(self) -> String {
        let beyond_the_message: Vec<String> = self
            .errors
            .into_iter()
            .filter(|error| *error != self.message)
            .collect();
        if beyond_the_message.is_empty() {
            self.message
        } else {
            format!("{}: {}", self.message, beyond_the_message.join("; "))
        }
    }
}

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused {
            Refused::Wrapped { error } => error,
            Refused::Bare(refusal) => refusal,
        }
    }
}

impl Api {
    #[must_use]
    pub fn new(http: reqwest::Client, config: &Config) -> Self {
        Self {
            http,
            base_url: config.base_url.clone(),
            token: config.token.clone(),
        }
    }

    fn request(&self, method: Method, path: &[&str]) -> RequestBuilder {
        self.http
            .request(method, endpoint(&self.base_url, path))
            .header(
                AUTHORIZATION,
                format!("Token token={}", self.token.expose_secret()),
            )
            .header(ACCEPT, VERSION_2)
            .timeout(REQUEST_TIMEOUT)
    }

    async fn every_page<T: DeserializeOwned>(
        &self,
        path: &[&str],
        filter: &[(&str, &str)],
    ) -> Result<Vec<T>, ApiError> {
        let mut pages = Vec::new();
        loop {
            let request = self
                .request(Method::GET, path)
                .query(filter)
                .query(&[("limit", PAGE_SIZE), ("offset", pages.len() * PAGE_SIZE)]);
            let Page { entries, more } = read(request).await?;
            pages.push(entries);
            if !more {
                return Ok(pages);
            }
        }
    }
}

pub(super) fn endpoint(base: &Url, path: &[&str]) -> Url {
    let mut endpoint = base.clone();
    if let Ok(mut segments) = endpoint.path_segments_mut() {
        segments.pop_if_empty().extend(path);
    }
    endpoint
}

pub(super) async fn accepted<R>(response: Response) -> Result<Response, ApiError>
where
    R: DeserializeOwned + Into<Refusal>,
{
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.bytes().await?;
    Err(serde_json::from_slice::<R>(&body).map_or_else(
        |_| ApiError::Rejected {
            status,
            reason: status.canonical_reason().unwrap_or(NO_REASON).to_owned(),
        },
        |refusal| refusal.into().rejecting(status),
    ))
}

async fn done(request: RequestBuilder) -> Result<(), ApiError> {
    accepted::<Refused>(request.send().await?).await.map(drop)
}

async fn read<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, ApiError> {
    let response = accepted::<Refused>(request.send().await?).await?;
    let status = response.status();
    let body = response.bytes().await?;
    serde_json::from_slice(&body).map_err(|cause| ApiError::Unreadable { status, cause })
}

const fn whole_seconds_in_utc(at: OffsetDateTime) -> OffsetDateTime {
    at.to_offset(UtcOffset::UTC).truncate_to_second()
}

#[cfg(test)]
mod tests;
