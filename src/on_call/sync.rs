use std::sync::Arc;

use tokio::sync::{Notify, watch};
use tokio::time::MissedTickBehavior;

use super::chosen_policies::ChosenPolicies;
use super::config::Config;
use super::roster::Roster;
use crate::pagerduty;

pub async fn keep_in_sync(
    pagerduty: pagerduty::Api,
    config: Config,
    roster: watch::Sender<Option<Roster>>,
    refresh: Arc<Notify>,
) {
    let mut refreshes = tokio::time::interval(config.refresh_interval);
    refreshes.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = refreshes.tick() => {}
            () = refresh.notified() => {}
        }
        match read(&pagerduty, &config.escalation_policies).await {
            Ok(read) => publish(&roster, read),
            Err(error) => {
                tracing::warn!(error = %error, cause = ?std::error::Error::source(&error), "keeping the previous roster");
            }
        }
    }
}

async fn read(
    pagerduty: &pagerduty::Api,
    chosen: &ChosenPolicies,
) -> Result<Roster, pagerduty::ApiError> {
    let (policies, oncalls) =
        tokio::try_join!(pagerduty.escalation_policies(), pagerduty.oncalls())?;
    tracing::debug!(
        policies = policies.len(),
        oncalls = oncalls.len(),
        "read the escalation policies and who is on call"
    );
    Ok(Roster::new(policies, oncalls, chosen))
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
