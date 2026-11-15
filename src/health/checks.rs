use reqwest::StatusCode;

use crate::pagerduty;
use crate::zulip;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Retry,
    Report,
}

#[derive(Debug)]
pub struct Failure {
    pub outcome: Outcome,
    pub error: String,
}

impl Failure {
    fn new(outcome: Outcome, error: impl Into<anyhow::Error>) -> Self {
        Self {
            outcome,
            error: format!("{:#}", error.into()),
        }
    }

    pub fn retry(error: impl Into<anyhow::Error>) -> Self {
        Self::new(Outcome::Retry, error)
    }
}

pub async fn test_pagerduty_token(pagerduty: &pagerduty::Api) -> Result<(), Failure> {
    pagerduty.abilities().await.map_err(|error| {
        let outcome = match &error {
            pagerduty::ApiError::Rejected { status, .. }
            | pagerduty::ApiError::ShiftBasedSchedule { status, .. }
            | pagerduty::ApiError::Unreadable { status, .. } => outcome_of_status(*status),
            pagerduty::ApiError::Request(_) => Outcome::Retry,
        };
        Failure::new(outcome, error)
    })
}

pub async fn find_zulip_bot(zulip: &zulip::Api) -> Result<(), Failure> {
    zulip.own_user().await.map(drop).map_err(|error| {
        let outcome = match &error {
            zulip::ApiError::Rejected { status, .. }
            | zulip::ApiError::Unreadable { status, .. } => outcome_of_status(*status),
            zulip::ApiError::Request(_) | zulip::ApiError::BadEventQueue => Outcome::Retry,
        };
        Failure::new(outcome, error)
    })
}

fn outcome_of_status(status: StatusCode) -> Outcome {
    match status {
        StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS => Outcome::Retry,
        status if status.is_client_error() => Outcome::Report,
        _ => Outcome::Retry,
    }
}
