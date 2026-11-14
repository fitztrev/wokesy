use nutype::nutype;

use super::schedule::ScheduleRef;
use super::user::UserRef;

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
pub struct EscalationPolicyId(String);

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
pub struct EscalationPolicyName(String);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Deref,
    Display,
    Deserialize
))]
pub struct EscalationLevel(u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    User(UserRef),
    Schedule(ScheduleRef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscalationRule {
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscalationPolicy {
    pub id: EscalationPolicyId,
    pub name: EscalationPolicyName,
    pub rules: Vec<EscalationRule>,
    pub in_use: bool,
}
