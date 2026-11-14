use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Notify, watch};
use tokio::time::MissedTickBehavior;

use super::roster::Roster;
use crate::pagerduty;

pub async fn keep_in_sync(
    pagerduty: pagerduty::Api,
    roster: watch::Sender<Option<Roster>>,
    refresh: Arc<Notify>,
    interval: Duration,
) {
    let mut refreshes = tokio::time::interval(interval);
    refreshes.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = refreshes.tick() => {}
            () = refresh.notified() => {}
        }
        match read(&pagerduty).await {
            Ok(read) => publish(&roster, read),
            Err(error) => {
                tracing::warn!(error = %error, cause = ?std::error::Error::source(&error), "keeping the previous roster");
            }
        }
    }
}

async fn read(pagerduty: &pagerduty::Api) -> Result<Roster, pagerduty::ApiError> {
    let (policies, oncalls) =
        tokio::try_join!(pagerduty.escalation_policies(), pagerduty.oncalls())?;
    tracing::debug!(
        policies = policies.len(),
        oncalls = oncalls.len(),
        "read the escalation policies and who is on call"
    );
    Ok(Roster::new(policies, oncalls))
}

fn publish(roster: &watch::Sender<Option<Roster>>, read: Roster) {
    let changed = roster.send_if_modified(|published| {
        if published.as_ref() == Some(&read) {
            return false;
        }
        *published = Some(read);
        true
    });
    if changed {
        tracing::info!("roster changed");
    }
}

#[cfg(test)]
mod tests;
