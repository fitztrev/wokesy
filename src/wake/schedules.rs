use serde::Deserialize;

use crate::pagerduty::ScheduleId;

const BETWEEN_SCHEDULES: char = ',';

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub struct Schedules(Vec<ScheduleId>);

impl Schedules {
    pub(super) fn ids(&self) -> &[ScheduleId] {
        &self.0
    }
}

impl From<&str> for Schedules {
    fn from(schedules: &str) -> Self {
        let mut distinct = Vec::new();
        let named = schedules
            .split(BETWEEN_SCHEDULES)
            .filter_map(|schedule| ScheduleId::try_new(schedule.trim()).ok());
        for schedule in named {
            if !distinct.contains(&schedule) {
                distinct.push(schedule);
            }
        }
        Self(distinct)
    }
}

impl From<String> for Schedules {
    fn from(schedules: String) -> Self {
        schedules.as_str().into()
    }
}

#[cfg(test)]
mod tests;
