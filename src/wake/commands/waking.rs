use std::time::Duration;

use humantime::format_duration;
use time::OffsetDateTime;
use tokio::time::Instant;

use super::own_overrides::{OwnOverride, WakeSchedule};
use super::{Commands, UNKNOWN_SENDER};
use crate::chat::{ChatUserId, time_tag};
use crate::on_call::{Level, Policy, Roster, name_in_chat, policy_in_chat};
use crate::pagerduty::{ApiError, ScheduleId, ScheduleName, UserId};
use crate::people::Directory;

const FOR_HOW_LONG: &str = "For how long? For example: `wake me for 4 hours`.";
const ON_NO_SCHEDULE: &str = ":warning: I couldn't put you on any schedule.";
const HANDED_BACK: &str = "PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.";
const NOT_AT_THE_FIRST_LINE: &str = "you are on it, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first.";
const FIRST_LINE: u32 = 1;
const BETWEEN_NAMES: &str = ", ";

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
        self.forget_expired();
        let mut lines = Vec::new();
        let mut on_some_schedule = false;
        for id in self.config.schedules.ids() {
            let schedule = WakeSchedule {
                id: id.clone(),
                name: name_of(id, roster),
            };
            let created = self
                .pagerduty
                .create_schedule_override(id, &user.id, stint.starts, stint.ends)
                .await;
            let outcome = match created {
                Ok(override_id) => {
                    tracing::info!(schedule = %id, override_id = %override_id, user = %user.id, "created an override");
                    self.own_overrides.push(OwnOverride {
                        schedule: schedule.clone(),
                        id: override_id,
                        user: user.id.clone(),
                        expires: stint.expires,
                    });
                    on_some_schedule = true;
                    on_it(id, roster)
                }
                Err(error) => {
                    tracing::warn!(schedule = %id, user = %user.id, error = %error, cause = ?std::error::Error::source(&error), "could not create an override");
                    if certainly_refused(&error) {
                        format!("I couldn't override it: {error}")
                    } else {
                        format!(
                            "I couldn't confirm the override: {error}. It may exist; check PagerDuty."
                        )
                    }
                }
            };
            lines.push(format!("- {}: {outcome}", schedule.in_chat()));
        }
        self.refresh.notify_one();
        woken(on_some_schedule.then_some(stint.ends), lines)
    }
}

fn certainly_refused(error: &ApiError) -> bool {
    matches!(error, ApiError::Rejected { status, .. } if status.is_client_error())
}

fn woken(until: Option<OffsetDateTime>, lines: Vec<String>) -> String {
    let heading = until.map_or_else(
        || ON_NO_SCHEDULE.to_owned(),
        |until| {
            format!(
                ":alarm_clock: I'll wake you for anything until {}.",
                time_tag(until)
            )
        },
    );
    std::iter::once(heading)
        .chain(lines)
        .chain(until.map(|_| HANDED_BACK.to_owned()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn name_of(schedule: &ScheduleId, roster: &Roster) -> Option<ScheduleName> {
    roster
        .policies
        .iter()
        .flat_map(|policy| &policy.levels)
        .flat_map(|level| &level.schedules)
        .find(|known| known.id == *schedule)
        .map(|known| known.name.clone())
}

fn on_it(schedule: &ScheduleId, roster: &Roster) -> String {
    let first_lines: Vec<(&Policy, &Level)> = roster
        .policies
        .iter()
        .filter_map(|policy| {
            let first_line = policy
                .levels
                .iter()
                .find(|level| level.number.into_inner() == FIRST_LINE)?;
            first_line
                .schedules
                .iter()
                .any(|target| target.id == *schedule)
                .then_some((policy, first_line))
        })
        .collect();
    if first_lines.is_empty() {
        return NOT_AT_THE_FIRST_LINE.to_owned();
    }
    let policies: Vec<String> = first_lines
        .iter()
        .map(|(policy, _)| policy_in_chat(&policy.name))
        .collect();
    let mut paged_directly: Vec<&UserId> = Vec::new();
    let mut names = Vec::new();
    for person in first_lines.iter().flat_map(|(_, level)| &level.people) {
        if !paged_directly.contains(&&person.id) {
            paged_directly.push(&person.id);
            names.push(name_in_chat(&person.name));
        }
    }
    let level_1_of = format!(
        "you are on it. It is level 1 of {}.",
        policies.join(BETWEEN_NAMES)
    );
    if names.is_empty() {
        level_1_of
    } else {
        format!(
            "{level_1_of} Level 1 also pages {} directly.",
            names.join(BETWEEN_NAMES)
        )
    }
}

#[cfg(test)]
mod tests;
