use serde::Deserialize;

use crate::pagerduty::EscalationPolicyId;

const BETWEEN_POLICIES: char = ',';

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub struct ChosenPolicies(Vec<EscalationPolicyId>);

impl ChosenPolicies {
    /// ```
    /// use wokesy::on_call::ChosenPolicies;
    /// use wokesy::pagerduty::EscalationPolicyId;
    ///
    /// let policy = |id: &str| EscalationPolicyId::try_new(id).unwrap();
    ///
    /// let some = ChosenPolicies::from("PANZZEQ, PT20YPA");
    /// assert!(some.include(&policy("PT20YPA")));
    /// assert!(!some.include(&policy("POTHER1")));
    ///
    /// let every = ChosenPolicies::default();
    /// assert!(every.include(&policy("POTHER1")));
    /// ```
    #[must_use]
    pub fn include(&self, policy: &EscalationPolicyId) -> bool {
        self.0.is_empty() || self.0.contains(policy)
    }
}

impl From<&str> for ChosenPolicies {
    fn from(policies: &str) -> Self {
        Self(
            policies
                .split(BETWEEN_POLICIES)
                .filter_map(|policy| EscalationPolicyId::try_new(policy.trim()).ok())
                .collect(),
        )
    }
}

impl From<String> for ChosenPolicies {
    fn from(policies: String) -> Self {
        policies.as_str().into()
    }
}

#[cfg(test)]
mod tests;
