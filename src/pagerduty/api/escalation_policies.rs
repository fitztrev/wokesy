use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::pagerduty::{
    EscalationPolicy, EscalationPolicyId, EscalationPolicyName, EscalationRule, ScheduleId,
    ScheduleName, ScheduleRef, Target, UserId, UserRef,
};
use crate::people::Name;

use super::{Api, ApiError};

#[derive(Deserialize)]
struct EscalationPoliciesAnswer {
    escalation_policies: Vec<EscalationPolicyAnswer>,
}

#[derive(Deserialize)]
struct EscalationPolicyAnswer {
    id: EscalationPolicyId,
    name: EscalationPolicyName,
    escalation_rules: Vec<EscalationRuleAnswer>,
    services: Vec<IgnoredAny>,
}

#[derive(Deserialize)]
struct EscalationRuleAnswer {
    targets: Vec<TargetAnswer>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum TargetAnswer {
    #[serde(rename = "user_reference", alias = "user")]
    User { id: UserId, summary: Name },
    #[serde(rename = "schedule_reference", alias = "schedule")]
    Schedule {
        id: ScheduleId,
        summary: ScheduleName,
    },
}

impl From<EscalationPolicyAnswer> for EscalationPolicy {
    fn from(answer: EscalationPolicyAnswer) -> Self {
        Self {
            id: answer.id,
            name: answer.name,
            rules: answer
                .escalation_rules
                .into_iter()
                .map(EscalationRule::from)
                .collect(),
            in_use: !answer.services.is_empty(),
        }
    }
}

impl From<EscalationRuleAnswer> for EscalationRule {
    fn from(answer: EscalationRuleAnswer) -> Self {
        Self {
            targets: answer.targets.into_iter().map(Target::from).collect(),
        }
    }
}

impl From<TargetAnswer> for Target {
    fn from(answer: TargetAnswer) -> Self {
        match answer {
            TargetAnswer::User { id, summary } => Self::User(UserRef { id, name: summary }),
            TargetAnswer::Schedule { id, summary } => {
                Self::Schedule(ScheduleRef { id, name: summary })
            }
        }
    }
}

impl Api {
    pub async fn escalation_policies(&self) -> Result<Vec<EscalationPolicy>, ApiError> {
        let pages: Vec<EscalationPoliciesAnswer> =
            self.every_page(&["escalation_policies"], &[]).await?;
        Ok(pages
            .into_iter()
            .flat_map(|page| page.escalation_policies)
            .map(EscalationPolicy::from)
            .collect())
    }
}
