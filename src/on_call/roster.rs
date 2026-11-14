use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use time::OffsetDateTime;

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
    /// use wokesy::on_call::Roster;
    /// use wokesy::pagerduty::{EscalationPolicy, EscalationPolicyId, EscalationPolicyName};
    ///
    /// let no_service_uses_it = EscalationPolicy {
    ///     id: EscalationPolicyId::try_new("PANZZEQ").unwrap(),
    ///     name: EscalationPolicyName::try_new("Engineering").unwrap(),
    ///     rules: vec![],
    ///     in_use: false,
    /// };
    ///
    /// assert_eq!(Roster::new(vec![no_service_uses_it], vec![]), Roster { policies: vec![] });
    /// ```
    #[must_use]
    pub fn new(policies: Vec<EscalationPolicy>, oncalls: Vec<OnCall>) -> Self {
        let mut shifts = shifts_by_level(oncalls);
        let mut policies: Vec<Policy> = policies
            .into_iter()
            .filter(|policy| policy.in_use)
            .map(|policy| Policy::of(policy, &mut shifts))
            .collect();
        policies.sort_by(|one, other| (&one.name, &one.id).cmp(&(&other.name, &other.id)));
        Self { policies }
    }

    /// ```
    /// use wokesy::on_call::Roster;
    /// use wokesy::pagerduty::{
    ///     EscalationPolicy, EscalationPolicyId, EscalationPolicyName, EscalationRule, ScheduleId,
    ///     ScheduleName, ScheduleRef, Target,
    /// };
    ///
    /// let schedule = |id: &str, name: &str| ScheduleRef {
    ///     id: ScheduleId::try_new(id).unwrap(),
    ///     name: ScheduleName::try_new(name).unwrap(),
    /// };
    /// let policy = EscalationPolicy {
    ///     id: EscalationPolicyId::try_new("PANZZEQ").unwrap(),
    ///     name: EscalationPolicyName::try_new("Engineering").unwrap(),
    ///     rules: vec![
    ///         EscalationRule { targets: vec![Target::Schedule(schedule("PI7DH85", "Primary rota"))] },
    ///         EscalationRule { targets: vec![Target::Schedule(schedule("PI7DH86", "Backup rota"))] },
    ///     ],
    ///     in_use: true,
    /// };
    ///
    /// let roster = Roster::new(vec![policy], vec![]);
    ///
    /// assert_eq!(roster.first_line_schedules(), [schedule("PI7DH85", "Primary rota")]);
    /// ```
    #[must_use]
    pub fn first_line_schedules(&self) -> Vec<ScheduleRef> {
        let mut distinct: Vec<ScheduleRef> = Vec::new();
        let first_line = self
            .policies
            .iter()
            .flat_map(|policy| &policy.levels)
            .filter(|level| level.number.into_inner() == FIRST_LINE)
            .flat_map(|level| &level.schedules);
        for schedule in first_line {
            if distinct.iter().all(|known| known.id != schedule.id) {
                distinct.push(schedule.clone());
            }
        }
        distinct
    }
}

impl Policy {
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
