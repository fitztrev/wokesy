use reqwest::Method;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};

use crate::pagerduty::{Override, OverrideId, ScheduleId, UserId};

use super::{Api, ApiError, Reference, done, read};

const SCHEDULES: &str = "schedules";
const OVERRIDES: &str = "overrides";

#[derive(Serialize)]
struct Between {
    #[serde(with = "time::serde::rfc3339")]
    since: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    until: OffsetDateTime,
}

#[derive(Deserialize)]
struct OverridesAnswer {
    overrides: Vec<OverrideAnswer>,
}

#[derive(Deserialize)]
struct OverrideAnswer {
    id: OverrideId,
    user: Reference<UserId>,
    #[serde(with = "time::serde::rfc3339")]
    start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    end: OffsetDateTime,
}

#[derive(Serialize)]
struct NewOverride<'a> {
    #[serde(rename = "override")]
    shift: Shift<'a>,
}

#[derive(Serialize)]
struct Shift<'a> {
    #[serde(with = "time::serde::rfc3339")]
    start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    end: OffsetDateTime,
    user: UserReference<'a>,
}

#[derive(Serialize)]
struct UserReference<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: ReferenceKind,
}

#[derive(Serialize)]
enum ReferenceKind {
    #[serde(rename = "user_reference")]
    User,
}

impl From<OverrideAnswer> for Override {
    fn from(answer: OverrideAnswer) -> Self {
        Self {
            id: answer.id,
            user: answer.user.id,
            start: answer.start,
            end: answer.end,
        }
    }
}

impl Api {
    pub async fn schedule_overrides(
        &self,
        schedule: &ScheduleId,
        since: OffsetDateTime,
        until: OffsetDateTime,
    ) -> Result<Vec<Override>, ApiError> {
        let between = Between {
            since: whole_seconds_in_utc(since),
            until: whole_seconds_in_utc(until),
        };
        let request = self
            .request(Method::GET, &[SCHEDULES, schedule, OVERRIDES])
            .query(&between);
        let answer: OverridesAnswer = read(request).await?;
        Ok(answer.overrides.into_iter().map(Override::from).collect())
    }

    pub async fn create_schedule_override(
        &self,
        schedule: &ScheduleId,
        user: &UserId,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<(), ApiError> {
        let new = NewOverride {
            shift: Shift {
                start: whole_seconds_in_utc(start),
                end: whole_seconds_in_utc(end),
                user: UserReference {
                    id: user,
                    kind: ReferenceKind::User,
                },
            },
        };
        let request = self
            .request(Method::POST, &[SCHEDULES, schedule, OVERRIDES])
            .json(&new);
        done(request).await
    }

    pub async fn delete_schedule_override(
        &self,
        schedule: &ScheduleId,
        id: &OverrideId,
    ) -> Result<(), ApiError> {
        done(self.request(Method::DELETE, &[SCHEDULES, schedule, OVERRIDES, id])).await
    }
}

const fn whole_seconds_in_utc(at: OffsetDateTime) -> OffsetDateTime {
    at.to_offset(UtcOffset::UTC).truncate_to_second()
}
