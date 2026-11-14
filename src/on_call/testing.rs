use time::OffsetDateTime;

use super::{Level, Policy, Roster, Shift};
use crate::pagerduty::{
    EscalationLevel, EscalationPolicyId, EscalationPolicyName, ScheduleId, ScheduleName,
    ScheduleRef, User, UserId, UserRef,
};
use crate::people::{Directory, Email, Links, Name};

pub fn user(id: &str, name: &str) -> UserRef {
    UserRef {
        id: UserId::try_new(id).unwrap(),
        name: Name::try_new(name).unwrap(),
    }
}

pub fn schedule(id: &str, name: &str) -> ScheduleRef {
    ScheduleRef {
        id: ScheduleId::try_new(id).unwrap(),
        name: ScheduleName::try_new(name).unwrap(),
    }
}

pub fn policy_id(id: &str) -> EscalationPolicyId {
    EscalationPolicyId::try_new(id).unwrap()
}

pub fn policy_name(name: &str) -> EscalationPolicyName {
    EscalationPolicyName::try_new(name).unwrap()
}

pub fn shift(user: UserRef, ends: Option<OffsetDateTime>) -> Shift {
    Shift { user, ends }
}

pub fn level(number: u32, shifts: &[Shift]) -> Level {
    Level {
        number: EscalationLevel::new(number),
        schedules: vec![],
        people: vec![],
        shifts: shifts.to_vec(),
    }
}

pub fn policy(id: &str, name: &str, levels: &[Level]) -> Policy {
    Policy {
        id: policy_id(id),
        name: policy_name(name),
        levels: levels.to_vec(),
    }
}

pub fn roster(policies: &[Policy]) -> Roster {
    Roster {
        policies: policies.to_vec(),
    }
}

pub fn directory(chat_users: &[(&UserRef, u64)]) -> Directory {
    let users: Vec<User> = chat_users
        .iter()
        .map(|(user, _)| User {
            id: user.id.clone(),
            name: user.name.clone(),
            email: email_of(user),
        })
        .collect();
    let links: Links = chat_users
        .iter()
        .map(|(user, chat_user)| format!("{}={chat_user}", email_of(user)))
        .collect::<Vec<_>>()
        .join(",")
        .parse()
        .unwrap();
    Directory::matching(&users, &[], &links)
}

fn email_of(user: &UserRef) -> Email {
    Email::try_new(format!("{}@example.com", user.id)).unwrap()
}
