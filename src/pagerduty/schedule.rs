use nutype::nutype;
use time::OffsetDateTime;

use super::user::UserId;

#[nutype(
    validate(not_empty),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Deref,
        Display,
        Deserialize
    )
)]
pub struct ScheduleId(String);

#[nutype(
    sanitize(trim),
    validate(not_empty),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Deref,
        Display,
        Deserialize
    )
)]
pub struct ScheduleName(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleRef {
    pub id: ScheduleId,
    pub name: ScheduleName,
}

#[nutype(
    validate(not_empty),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Deref,
        Display,
        Deserialize
    )
)]
pub struct OverrideId(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Override {
    pub id: OverrideId,
    pub user: UserId,
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
}
