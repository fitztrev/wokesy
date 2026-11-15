use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use time::OffsetDateTime;

use super::chosen_policies::ChosenPolicies;
use crate::pagerduty::{
    EscalationLevel, EscalationPolicy, EscalationPolicyId, EscalationPolicyName, EscalationRule,
    OnCall, ScheduleRef, Target, UserId, UserRef,
};

const FIRST_LINE: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roster {
    pub policies: Vec<Policy>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub id: EscalationPolicyId,
    pub name: EscalationPolicyName,
    pub levels: Vec<Level>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    pub number: EscalationLevel,
    pub schedules: Vec<ScheduleRef>,
    pub people: Vec<UserRef>,
    pub shifts: Vec<Shift>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shift {
    pub user: UserRef,
    pub ends: Option<OffsetDateTime>,
}

type ShiftsByLevel = BTreeMap<(EscalationPolicyId, EscalationLevel), BTreeMap<UserId, Shift>>;

impl Roster {
    /// ```
    /// use wokesy::on_call::{ChosenPolicies, Roster};
    /// use wokesy::pagerduty::{EscalationPolicy, EscalationPolicyId, EscalationPolicyName};
    ///
    /// let policy = |id: &str, in_use: bool| EscalationPolicy {
    ///     id: EscalationPolicyId::try_new(id).unwrap(),
    ///     name: EscalationPolicyName::try_new("Engineering").unwrap(),
    ///     rules: vec![],
    ///     in_use,
    /// };
    /// let in_use = |roster: Roster| -> Vec<String> {
    ///     roster.policies.iter().map(|policy| policy.id.to_string()).collect()
    /// };
    /// let policies = || vec![policy("PANZZEQ", true), policy("PT20YPA", true), policy("PUNUSED", false)];
    ///
    /// let every = Roster::new(policies(), vec![], &ChosenPolicies::default());
    /// let chosen = Roster::new(policies(), vec![], &"PT20YPA, PUNUSED".into());
    ///
    /// assert_eq!(in_use(every), ["PANZZEQ", "PT20YPA"]);
    /// assert_eq!(in_use(chosen), ["PT20YPA"]);
    /// ```
    #[must_use]
    pub fn new(
        policies: Vec<EscalationPolicy>,
        oncalls: Vec<OnCall>,
        chosen: &ChosenPolicies,
    ) -> Self {
        let mut shifts = shifts_by_level(oncalls);
        let mut policies: Vec<Policy> = policies
            .into_iter()
            .filter(|policy| policy.in_use && chosen.include(&policy.id))
            .map(|policy| Policy::of(policy, &mut shifts))
            .collect();
        policies.sort_by(|one, other| (&one.name, &one.id).cmp(&(&other.name, &other.id)));
        Self { policies }
    }
}

impl Policy {
    /// ```
    /// use wokesy::on_call::{Level, Policy, Shift};
    /// use wokesy::pagerduty::{
    ///     EscalationLevel, EscalationPolicyId, EscalationPolicyName, UserId, UserRef,
    /// };
    /// use wokesy::people::Name;
    ///
    /// let carol = Shift {
    ///     user: UserRef {
    ///         id: UserId::try_new("PCAROL1").unwrap(),
    ///         name: Name::try_new("Carol Example").unwrap(),
    ///     },
    ///     ends: None,
    /// };
    /// let level = |number: u32, shifts: &[Shift]| Level {
    ///     number: EscalationLevel::new(number),
    ///     schedules: vec![],
    ///     people: vec![],
    ///     shifts: shifts.to_vec(),
    /// };
    /// let policy = |levels: Vec<Level>| Policy {
    ///     id: EscalationPolicyId::try_new("PANZZEQ").unwrap(),
    ///     name: EscalationPolicyName::try_new("Engineering").unwrap(),
    ///     levels,
    /// };
    ///
    /// let skipping_the_first = policy(vec![level(1, &[]), level(2, &[carol.clone()]), level(3, &[carol])]);
    /// let with_nobody = policy(vec![level(1, &[])]);
    ///
    /// assert_eq!(skipping_the_first.alerted_first().map(|level| level.number.into_inner()), Some(2));
    /// assert_eq!(with_nobody.alerted_first(), None);
    /// ```
    #[must_use]
    pub fn alerted_first(&self) -> Option<&Level> {
        self.levels.iter().find(|level| !level.shifts.is_empty())
    }

    fn of(policy: EscalationPolicy, shifts: &mut ShiftsByLevel) -> Self {
        let levels = (FIRST_LINE..)
            .map(EscalationLevel::new)
            .zip(policy.rules)
            .map(|(number, rule)| {
                let on_call = shifts
                    .remove(&(policy.id.clone(), number))
                    .unwrap_or_default();
                Level::of(number, rule, on_call.into_values().collect())
            })
            .collect();
        Self {
            id: policy.id,
            name: policy.name,
            levels,
        }
    }
}

impl Level {
    fn of(number: EscalationLevel, rule: EscalationRule, shifts: Vec<Shift>) -> Self {
        let mut schedules = Vec::new();
        let mut people = Vec::new();
        for target in rule.targets {
            match target {
                Target::Schedule(schedule) => schedules.push(schedule),
                Target::User(user) => people.push(user),
            }
        }
        Self {
            number,
            schedules,
            people,
            shifts,
        }
    }
}

impl Shift {
    fn ends_later_than(&self, other: &Self) -> bool {
        match (self.ends, other.ends) {
            (Some(ends), Some(other_ends)) => ends > other_ends,
            (None, Some(_)) => true,
            (_, None) => false,
        }
    }
}

fn shifts_by_level(oncalls: Vec<OnCall>) -> ShiftsByLevel {
    let mut shifts = ShiftsByLevel::new();
    for on_call in oncalls {
        let shift = Shift {
            user: on_call.user,
            ends: on_call.end,
        };
        let at_level = shifts
            .entry((on_call.escalation_policy, on_call.level))
            .or_default();
        match at_level.entry(shift.user.id.clone()) {
            Entry::Vacant(unlisted) => {
                unlisted.insert(shift);
            }
            Entry::Occupied(mut listed) => {
                if shift.ends_later_than(listed.get()) {
                    listed.insert(shift);
                }
            }
        }
    }
    shifts
}

#[cfg(test)]
mod tests;
