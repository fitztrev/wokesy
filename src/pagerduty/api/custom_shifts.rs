use reqwest::Method;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::pagerduty::{CustomShiftId, ScheduleId, UserId};

use super::{
    Api, ApiError, LONGEST_WAIT_FOR_A_CHANGE, Reference, done, read, whole_seconds_in_utc,
};

const VERSION_3: &str = "v3";
const SCHEDULES: &str = "schedules";
const CUSTOM_SHIFTS: &str = "custom_shifts";

#[derive(Serialize)]
struct NewCustomShifts<'a> {
    custom_shifts: [NewCustomShift<'a>; 1],
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "custom_shift")]
struct NewCustomShift<'a> {
    #[serde(with = "time::serde::rfc3339")]
    start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    end_time: OffsetDateTime,
    assignments: [Assignment<'a>; 1],
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "shift_assignment")]
struct Assignment<'a> {
    member: Member<'a>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "user_member")]
struct Member<'a> {
    user_id: &'a str,
}

#[derive(Deserialize)]
#[serde(try_from = "CreatedAnswer")]
struct Created(CustomShiftId);

#[derive(Deserialize)]
struct CreatedAnswer {
    custom_shifts: Vec<Reference<CustomShiftId>>,
}

#[derive(Debug, thiserror::Error)]
#[error("the answer lists no custom shift")]
struct NothingCreated;

impl TryFrom<CreatedAnswer> for Created {
    type Error = NothingCreated;

    fn try_from(answer: CreatedAnswer) -> Result<Self, Self::Error> {
        answer
            .custom_shifts
            .into_iter()
            .next()
            .map(|created| Self(created.id))
            .ok_or(NothingCreated)
    }
}

impl Api {
    pub async fn create_custom_shift(
        &self,
        schedule: &ScheduleId,
        user: &UserId,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<CustomShiftId, ApiError> {
        let new = NewCustomShifts {
            custom_shifts: [NewCustomShift {
                start_time: whole_seconds_in_utc(start),
                end_time: whole_seconds_in_utc(end),
                assignments: [Assignment {
                    member: Member { user_id: user },
                }],
            }],
        };
        let request = self
            .request(
                Method::POST,
                &[VERSION_3, SCHEDULES, schedule, CUSTOM_SHIFTS],
            )
            .timeout(LONGEST_WAIT_FOR_A_CHANGE)
            .json(&new);
        let Created(id) = read(request).await?;
        Ok(id)
    }

    pub async fn delete_custom_shift(
        &self,
        schedule: &ScheduleId,
        id: &CustomShiftId,
    ) -> Result<(), ApiError> {
        let request = self
            .request(
                Method::DELETE,
                &[VERSION_3, SCHEDULES, schedule, CUSTOM_SHIFTS, id],
            )
            .timeout(LONGEST_WAIT_FOR_A_CHANGE);
        done(request).await
    }
}
