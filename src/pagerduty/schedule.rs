use nutype::nutype;

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
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, Deserialize)
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
pub struct CustomShiftId(String);
