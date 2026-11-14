use serde::Deserialize;
use time::OffsetDateTime;

use crate::pagerduty::{EscalationLevel, EscalationPolicyId, OnCall, ScheduleId, UserId, UserRef};
use crate::people::Name;

use super::{Api, ApiError, Reference};

const ONLY_THE_EARLIEST: [(&str, &str); 1] = [("earliest", "true")];

#[derive(Deserialize)]
struct OnCallsAnswer {
    oncalls: Vec<OnCallAnswer>,
}

#[derive(Deserialize)]
struct OnCallAnswer {
    escalation_policy: Reference<EscalationPolicyId>,
    escalation_level: EscalationLevel,
    schedule: Option<Reference<ScheduleId>>,
    user: UserAnswer,
    #[serde(default, with = "time::serde::rfc3339::option")]
    end: Option<OffsetDateTime>,
}

#[derive(Deserialize)]
struct UserAnswer {
    id: UserId,
    summary: Name,
}

impl From<OnCallAnswer> for OnCall {
    fn from(answer: OnCallAnswer) -> Self {
        Self {
            escalation_policy: answer.escalation_policy.id,
            level: answer.escalation_level,
            schedule: answer.schedule.map(|schedule| schedule.id),
            user: UserRef {
                id: answer.user.id,
                name: answer.user.summary,
            },
            end: answer.end,
        }
    }
}

impl Api {
    pub async fn oncalls(&self) -> Result<Vec<OnCall>, ApiError> {
        let pages: Vec<OnCallsAnswer> = self.every_page(&["oncalls"], &ONLY_THE_EARLIEST).await?;
        Ok(pages
            .into_iter()
            .flat_map(|page| page.oncalls)
            .map(OnCall::from)
            .collect())
    }
}
