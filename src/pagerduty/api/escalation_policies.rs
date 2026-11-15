use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::pagerduty::{
    EscalationPolicy, EscalationPolicyId, EscalationPolicyName, EscalationRule, ScheduleId,
    ScheduleName, ScheduleRef, Target, UserId, UserRef,
};
use crate::people::Name;

use super::{Api, ApiError, Entry, id_of};

#[derive(Deserialize)]
struct EscalationPoliciesAnswer {
    escalation_policies: Vec<Entry<EscalationPolicyAnswer>>,
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
    targets: Vec<Entry<TargetAnswer>>,
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
        let rules = answer
            .escalation_rules
            .into_iter()
            .map(|rule| rule.read_for(&answer.id))
            .collect();
        Self {
            id: answer.id,
            name: answer.name,
            rules,
            in_use: !answer.services.is_empty(),
        }
    }
}

impl EscalationRuleAnswer {
    fn read_for(self, policy: &EscalationPolicyId) -> EscalationRule {
        let targets = self
            .targets
            .into_iter()
            .filter_map(|target| {
                target.read_or_else(|unreadable| {
                    tracing::warn!(
                        escalation_policy = %policy,
                        target = id_of(unreadable),
                        "pagerduty escalation target that cannot be read skipped"
                    );
                })
            })
            .map(Target::from)
            .collect();
        EscalationRule { targets }
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
            .filter_map(|policy| {
                policy.read_or_else(|unreadable| {
                    tracing::warn!(
                        escalation_policy = id_of(unreadable),
                        "pagerduty escalation policy that cannot be read skipped"
                    );
                })
            })
            .map(EscalationPolicy::from)
            .collect())
    }
}
