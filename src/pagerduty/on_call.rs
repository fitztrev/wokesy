use time::OffsetDateTime;

use super::escalation_policy::{EscalationLevel, EscalationPolicyId};
use super::schedule::ScheduleId;
use super::user::UserRef;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnCall {
    pub escalation_policy: EscalationPolicyId,
    pub level: EscalationLevel,
    pub schedule: Option<ScheduleId>,
    pub user: UserRef,
    pub end: Option<OffsetDateTime>,
}
