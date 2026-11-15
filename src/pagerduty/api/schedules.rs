use std::time::Duration;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};

use crate::pagerduty::{OverrideId, ScheduleId, UserId};

use super::{Api, ApiError, Reference, done, read};

const SCHEDULES: &str = "schedules";
const OVERRIDES: &str = "overrides";
const LONGEST_WAIT_FOR_AN_OVERRIDE: Duration = Duration::from_secs(10);

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

#[derive(Deserialize)]
#[serde(try_from = "CreatedAnswer")]
struct Created(OverrideId);

#[derive(Deserialize)]
#[serde(untagged)]
enum CreatedAnswer {
    Alone(CreatedOverride),
    Listed(Vec<CreatedOverride>),
}

#[derive(Deserialize)]
struct CreatedOverride {
    #[serde(rename = "override")]
    created: Reference<OverrideId>,
}

#[derive(Debug, thiserror::Error)]
#[error("the answer lists no override")]
struct NothingCreated;

impl TryFrom<CreatedAnswer> for Created {
    type Error = NothingCreated;

    fn try_from(answer: CreatedAnswer) -> Result<Self, Self::Error> {
        let first = match answer {
            CreatedAnswer::Alone(only) => Some(only),
            CreatedAnswer::Listed(listed) => listed.into_iter().next(),
        };
        first
            .map(|answer| Self(answer.created.id))
            .ok_or(NothingCreated)
    }
}

impl Api {
    pub async fn create_schedule_override(
        &self,
        schedule: &ScheduleId,
        user: &UserId,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<OverrideId, ApiError> {
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
            .timeout(LONGEST_WAIT_FOR_AN_OVERRIDE)
            .json(&new);
        let Created(id) = read(request).await?;
        Ok(id)
    }

    pub async fn delete_schedule_override(
        &self,
        schedule: &ScheduleId,
        id: &OverrideId,
    ) -> Result<(), ApiError> {
        let request = self
            .request(Method::DELETE, &[SCHEDULES, schedule, OVERRIDES, id])
            .timeout(LONGEST_WAIT_FOR_AN_OVERRIDE);
        done(request).await
    }
}

const fn whole_seconds_in_utc(at: OffsetDateTime) -> OffsetDateTime {
    at.to_offset(UtcOffset::UTC).truncate_to_second()
}
