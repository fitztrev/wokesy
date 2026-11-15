use reqwest::StatusCode;
use tokio::time::Instant;

use super::{Commands, UNKNOWN_SENDER};
use crate::chat::{ChatUserId, silenced};
use crate::on_call::{name_in_chat, schedule_in_chat};
use crate::pagerduty::{ApiError, OverrideId, ScheduleRef, UserId};
use crate::people::Directory;

const NO_OVERRIDE_OF_YOURS: &str = "I have no override of yours on record. I only end overrides I made myself, and I forget them when I restart. Anything I made earlier still ends on time by itself, or you can remove it in PagerDuty.";
const NOTHING_TO_UNDO: &str = "Nothing to undo: I have no override of mine on record. I forget them when I restart; anything I made earlier still ends on time by itself.";
const BACK_TO_THE_DEFAULTS: &str = ":leftwards_arrow_with_hook: Back to the defaults.";
const PAGERDUTY_DECIDES_AGAIN: &str = "PagerDuty's own schedules and overrides decide again.";
const BETWEEN_NAMES: &str = ", ";
const BETWEEN_FAILURES: &str = "; ";

#[derive(Clone, Debug)]
pub(super) struct OwnOverride {
    pub schedule: ScheduleRef,
    pub id: OverrideId,
    pub user: UserId,
    pub expires: Instant,
}

struct Ending {
    schedule: ScheduleRef,
    user: UserId,
    failure: Option<String>,
}

impl Commands {
    pub(super) fn forget_expired(&mut self) {
        let now = Instant::now();
        self.own_overrides.retain(|own| own.expires > now);
    }

    pub(super) async fn stop(&mut self, sender: ChatUserId, directory: &Directory) -> String {
        let sender = directory.pagerduty_user_of(sender).map(|user| &user.id);
        let endings = self.end(|own| Some(&own.user) == sender).await;
        if endings.is_empty() {
            return NO_OVERRIDE_OF_YOURS.to_owned();
        }
        stopped(&endings)
    }

    pub(super) async fn reset(&mut self, sender: ChatUserId, directory: &Directory) -> String {
        if directory.pagerduty_user_of(sender).is_none() {
            return UNKNOWN_SENDER.to_owned();
        }
        let endings = self.end(|_| true).await;
        if endings.is_empty() {
            return NOTHING_TO_UNDO.to_owned();
        }
        back_to_the_defaults(&endings, directory)
    }

    async fn end(&mut self, chosen: impl Fn(&OwnOverride) -> bool) -> Vec<Ending> {
        self.forget_expired();
        let (ending, kept): (Vec<OwnOverride>, Vec<OwnOverride>) =
            std::mem::take(&mut self.own_overrides)
                .into_iter()
                .partition(chosen);
        self.own_overrides = kept;
        let mut endings = Vec::with_capacity(ending.len());
        for own in ending {
            let failure = match self.delete(&own).await {
                Ok(()) => {
                    tracing::info!(schedule = %own.schedule.id, override_id = %own.id, user = %own.user, "ended an override");
                    None
                }
                Err(error) => {
                    tracing::warn!(schedule = %own.schedule.id, override_id = %own.id, user = %own.user, error = %error, cause = ?std::error::Error::source(&error), "could not end an override");
                    Some(error.to_string())
                }
            };
            let ending = Ending {
                schedule: own.schedule.clone(),
                user: own.user.clone(),
                failure,
            };
            if ending.failure.is_some() {
                self.own_overrides.push(own);
            }
            endings.push(ending);
        }
        if !endings.is_empty() {
            self.refresh.notify_one();
        }
        endings
    }

    async fn delete(&self, own: &OwnOverride) -> Result<(), ApiError> {
        let deleted = self
            .pagerduty
            .delete_schedule_override(&own.schedule.id, &own.id)
            .await;
        match deleted {
            Err(ApiError::Rejected {
                status: StatusCode::NOT_FOUND,
                ..
            }) => Ok(()),
            answered => answered,
        }
    }
}

fn stopped(endings: &[Ending]) -> String {
    let ended = distinct(
        endings
            .iter()
            .filter(|ending| ending.failure.is_none())
            .map(|ending| schedule_in_chat(&ending.schedule.name)),
    );
    let ended = (!ended.is_empty()).then(|| {
        format!(
            ":zzz: Ended your override on {}. The schedule decides again.",
            ended.join(BETWEEN_NAMES)
        )
    });
    let failed = distinct(endings.iter().filter_map(|ending| {
        let error = ending.failure.as_ref()?;
        Some(format!(
            "I couldn't end your override on {}: {error}",
            schedule_in_chat(&ending.schedule.name)
        ))
    }));
    ended
        .into_iter()
        .chain(failed)
        .collect::<Vec<_>>()
        .join("\n")
}

fn back_to_the_defaults(endings: &[Ending], directory: &Directory) -> String {
    let schedules = distinct(endings.iter().map(|ending| &ending.schedule));
    let lines = schedules.into_iter().map(|schedule| {
        let on_schedule: Vec<&Ending> = endings
            .iter()
            .filter(|ending| ending.schedule.id == schedule.id)
            .collect();
        format!(
            "- {}: {}",
            schedule_in_chat(&schedule.name),
            outcome(&on_schedule, directory)
        )
    });
    std::iter::once(BACK_TO_THE_DEFAULTS.to_owned())
        .chain(lines)
        .chain(std::iter::once(PAGERDUTY_DECIDES_AGAIN.to_owned()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn outcome(on_schedule: &[&Ending], directory: &Directory) -> String {
    let name = |user: &UserId| {
        directory
            .name_of(user)
            .map_or_else(|| silenced(user), name_in_chat)
    };
    let ended = distinct(
        on_schedule
            .iter()
            .filter(|ending| ending.failure.is_none())
            .map(|ending| name(&ending.user)),
    );
    let ended = (!ended.is_empty())
        .then(|| format!("ended the override of {}.", ended.join(BETWEEN_NAMES)));
    let failed = distinct(on_schedule.iter().filter_map(|ending| {
        let error = ending.failure.as_ref()?;
        Some(format!(
            "I couldn't end the override of {}: {error}",
            name(&ending.user)
        ))
    }));
    let failed = (!failed.is_empty()).then(|| failed.join(BETWEEN_FAILURES));
    ended
        .into_iter()
        .chain(failed)
        .collect::<Vec<_>>()
        .join(" ")
}

fn distinct<T: PartialEq>(all: impl Iterator<Item = T>) -> Vec<T> {
    let mut distinct = Vec::new();
    for one in all {
        if !distinct.contains(&one) {
            distinct.push(one);
        }
    }
    distinct
}

#[cfg(test)]
mod tests;
