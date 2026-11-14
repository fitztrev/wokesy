use std::collections::BTreeMap;
use std::time::Duration;

use humantime::format_duration;
use time::OffsetDateTime;
use tokio::time::Instant;

use super::own_overrides::OwnOverride;
use super::{Commands, UNKNOWN_SENDER};
use crate::chat::{ChatUserId, time_tag};
use crate::on_call::{Policy, Roster};
use crate::pagerduty::{ScheduleId, ScheduleRef, UserRef};
use crate::people::Directory;

const FOR_HOW_LONG: &str = "For how long? For example: `wake me for 4 hours`.";
const NOTHING_TO_WAKE_FOR: &str =
    "No escalation policy is used by a service, so there is nothing to wake you for.";
const FIRST_NOWHERE: &str = ":warning: I couldn't put you first anywhere.";
const HANDED_BACK: &str = "PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.";
const NO_SCHEDULE: &str = "I can't put you first. Level 1 has no schedule to override.";
const FIRST_LINE: u32 = 1;
const BETWEEN_NAMES: &str = ", ";
const BETWEEN_REFUSALS: &str = "; ";

type Refusals = BTreeMap<ScheduleId, String>;

struct Stint {
    starts: OffsetDateTime,
    ends: OffsetDateTime,
    expires: Instant,
}

impl Stint {
    fn starting_now(lasting: Duration) -> Option<Self> {
        let starts = OffsetDateTime::now_utc();
        Some(Self {
            starts,
            ends: starts.checked_add(lasting.try_into().ok()?)?,
            expires: Instant::now().checked_add(lasting)?,
        })
    }
}

impl Commands {
    pub(super) async fn wake(
        &mut self,
        sender: ChatUserId,
        lasting: Option<Duration>,
        roster: &Roster,
        directory: &Directory,
    ) -> String {
        let Some(user) = directory.pagerduty_user_of(sender) else {
            return UNKNOWN_SENDER.to_owned();
        };
        let Some(lasting) = lasting.filter(|lasting| !lasting.is_zero()) else {
            return FOR_HOW_LONG.to_owned();
        };
        let allowed = (lasting <= self.config.longest)
            .then(|| Stint::starting_now(lasting))
            .flatten();
        let Some(stint) = allowed else {
            return format!(
                "That is longer than I'm allowed to set ({}). Ask for less, or change the schedule in PagerDuty.",
                format_duration(self.config.longest)
            );
        };
        if roster.policies.is_empty() {
            return NOTHING_TO_WAKE_FOR.to_owned();
        }
        self.forget_expired();
        let schedules = roster.first_line_schedules();
        let mut refusals = Refusals::new();
        for schedule in &schedules {
            let created = self
                .pagerduty
                .create_schedule_override(&schedule.id, &user.id, stint.starts, stint.ends)
                .await;
            match created {
                Ok(id) => {
                    tracing::info!(schedule = %schedule.id, override_id = %id, user = %user.id, "created an override");
                    self.own_overrides.push(OwnOverride {
                        schedule: schedule.clone(),
                        id,
                        user: user.id.clone(),
                        expires: stint.expires,
                    });
                }
                Err(error) => {
                    tracing::warn!(schedule = %schedule.id, user = %user.id, error = %error, cause = ?std::error::Error::source(&error), "could not create an override");
                    refusals.insert(schedule.id.clone(), error.to_string());
                }
            }
        }
        self.refresh.notify_one();
        let first_somewhere = refusals.len() < schedules.len();
        woken(roster, &refusals, first_somewhere.then_some(stint.ends))
    }
}

fn woken(roster: &Roster, refusals: &Refusals, until: Option<OffsetDateTime>) -> String {
    let heading = until.map_or_else(
        || FIRST_NOWHERE.to_owned(),
        |until| {
            format!(
                ":alarm_clock: I'll wake you for anything until {}.",
                time_tag(until)
            )
        },
    );
    std::iter::once(heading)
        .chain(roster.policies.iter().map(|policy| line(policy, refusals)))
        .chain(until.map(|_| HANDED_BACK.to_owned()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn line(policy: &Policy, refusals: &Refusals) -> String {
    let first_line = policy
        .levels
        .iter()
        .find(|level| level.number.into_inner() == FIRST_LINE);
    let (schedules, people): (&[ScheduleRef], &[UserRef]) = first_line
        .map_or((&[], &[]), |level| {
            (level.schedules.as_slice(), level.people.as_slice())
        });
    let outcome = if schedules.is_empty() {
        NO_SCHEDULE.to_owned()
    } else {
        outcome(schedules, people, refusals)
    };
    format!("- **{}**: {outcome}", policy.name)
}

fn outcome(schedules: &[ScheduleRef], people: &[UserRef], refusals: &Refusals) -> String {
    let overridden: Vec<String> = schedules
        .iter()
        .filter(|schedule| !refusals.contains_key(&schedule.id))
        .map(|schedule| format!("**{}**", schedule.name))
        .collect();
    let refused: Vec<String> = schedules
        .iter()
        .filter_map(|schedule| {
            let error = refusals.get(&schedule.id)?;
            Some(format!(
                "I couldn't override **{}**: {error}",
                schedule.name
            ))
        })
        .collect();
    let mut sentences = Vec::new();
    if !overridden.is_empty() {
        sentences.push(format!(
            "you are first in line through {}.",
            overridden.join(BETWEEN_NAMES)
        ));
        if !people.is_empty() {
            let names: Vec<&str> = people.iter().map(|person| person.name.as_str()).collect();
            sentences.push(format!(
                "{} stay at level 1 too.",
                names.join(BETWEEN_NAMES)
            ));
        }
    }
    if !refused.is_empty() {
        sentences.push(refused.join(BETWEEN_REFUSALS));
    }
    sentences.join(" ")
}

#[cfg(test)]
mod tests;
