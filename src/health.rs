mod checks;
mod config;

use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use crate::pagerduty;
use crate::zulip;
use checks::{Failure, Outcome, find_zulip_bot, test_pagerduty_token};
pub use config::Config;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Health {
    pub pagerduty: Availability,
    pub zulip: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable(Unavailable),
}

impl Availability {
    #[must_use]
    pub const fn unavailable(&self) -> Option<&Unavailable> {
        match self {
            Self::Available => None,
            Self::Unavailable(unavailable) => Some(unavailable),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    pub recovery: Recovery,
    pub error: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    Recoverable,
    NeedsAPerson,
}

pub async fn keep_checking(
    config: Config,
    pagerduty: pagerduty::Api,
    zulip: zulip::Api,
    health: watch::Sender<Option<Health>>,
) {
    let mut ticks = tokio::time::interval(config.interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            () = health.closed() => return,
            _ = ticks.tick() => {}
        }
        let (pagerduty, zulip) = tokio::join!(
            check(config, "PagerDuty", || test_pagerduty_token(&pagerduty)),
            check(config, "Zulip", || find_zulip_bot(&zulip)),
        );
        publish(&health, Health { pagerduty, zulip });
    }
}

async fn check<F: Future<Output = Result<(), Failure>>>(
    config: Config,
    service: &str,
    mut check: impl FnMut() -> F,
) -> Availability {
    let timeout = config.timeout;
    let backoff = ExponentialBuilder::new()
        .with_min_delay(config.initial_backoff)
        .without_max_delay()
        .with_max_times(config.retries);
    let checked = (|| within(timeout, check()))
        .retry(backoff)
        .sleep(tokio::time::sleep)
        .when(|failure| failure.outcome == Outcome::Retry)
        .notify(|failure, delay| {
            tracing::warn!(%service, error = %failure.error, retry_in = ?delay, "health check failed");
        })
        .await;
    availability(checked)
}

async fn within(
    timeout: Duration,
    check: impl Future<Output = Result<(), Failure>>,
) -> Result<(), Failure> {
    tokio::time::timeout(timeout, check)
        .await
        .unwrap_or_else(|_| {
            Err(Failure::retry(anyhow::anyhow!(
                "no answer within {timeout:?}"
            )))
        })
}

fn availability(checked: Result<(), Failure>) -> Availability {
    match checked {
        Ok(()) => Availability::Available,
        Err(Failure { outcome, error }) => Availability::Unavailable(Unavailable {
            recovery: match outcome {
                Outcome::Retry => Recovery::Recoverable,
                Outcome::Report => Recovery::NeedsAPerson,
            },
            error,
        }),
    }
}

fn publish(published: &watch::Sender<Option<Health>>, health: Health) {
    if published.borrow().as_ref() != Some(&health) {
        tracing::info!(?health, "health changed");
        published.send_replace(Some(health));
    }
}

#[cfg(test)]
mod tests;
