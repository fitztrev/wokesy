use time::macros::datetime;

use super::*;
use crate::on_call::testing::{policy_id, policy_name, schedule, user};

const EVENING: OffsetDateTime = datetime!(2026-11-14 20:00 UTC);
const MIDNIGHT: OffsetDateTime = datetime!(2026-11-15 00:00 UTC);

fn escalation_policy(id: &str, name: &str, rules: &[&[Target]]) -> EscalationPolicy {
    EscalationPolicy {
        id: policy_id(id),
        name: policy_name(name),
        rules: rules
            .iter()
            .map(|targets| EscalationRule {
                targets: targets.to_vec(),
            })
            .collect(),
        in_use: true,
    }
}

fn unused(policy: EscalationPolicy) -> EscalationPolicy {
    EscalationPolicy {
        in_use: false,
        ..policy
    }
}

fn on_call(policy: &str, level: u32, user: UserRef, end: Option<OffsetDateTime>) -> OnCall {
    OnCall {
        escalation_policy: policy_id(policy),
        level: EscalationLevel::new(level),
        schedule: None,
        user,
        end,
    }
}

fn alice() -> UserRef {
    user("PALICE1", "Alice Example")
}

fn bob() -> UserRef {
    user("PBOB001", "Bob Example")
}

fn carol() -> UserRef {
    user("PCAROL1", "Carol Example")
}

fn primary() -> ScheduleRef {
    schedule("PI7DH85", "Primary rota")
}

fn backup() -> ScheduleRef {
    schedule("PI7DH86", "Backup rota")
}

fn roster_of(policies: Vec<EscalationPolicy>, oncalls: Vec<OnCall>) -> Roster {
    Roster::new(policies, oncalls, &ChosenPolicies::default())
}

fn shifts_of(roster: &Roster, policy: usize, level: usize) -> Vec<Shift> {
    roster.policies[policy].levels[level].shifts.clone()
}

#[test]
fn only_policies_a_service_uses_are_in_the_roster() {
    let roster = roster_of(
        vec![
            unused(escalation_policy("PUNUSED", "Abandoned", &[&[]])),
            escalation_policy("PLICHES", "Lichess", &[&[]]),
        ],
        vec![
            on_call("PUNUSED", 1, alice(), None),
            on_call("PLICHES", 1, bob(), None),
        ],
    );

    assert_eq!(
        roster,
        Roster {
            policies: vec![Policy {
                id: policy_id("PLICHES"),
                name: policy_name("Lichess"),
                levels: vec![Level {
                    number: EscalationLevel::new(1),
                    schedules: vec![],
                    people: vec![],
                    shifts: vec![Shift {
                        user: bob(),
                        ends: None
                    }],
                }],
            }],
        }
    );
}

#[test]
fn with_policies_chosen_only_those_among_the_policies_in_use_are_in_the_roster() {
    let roster = Roster::new(
        vec![
            escalation_policy("PLICHES", "Lichess", &[&[]]),
            escalation_policy("PMODERA", "Moderation", &[&[]]),
            unused(escalation_policy("PUNUSED", "Abandoned", &[&[]])),
            escalation_policy("PFISHNE", "Fishnet", &[&[]]),
        ],
        vec![
            on_call("PLICHES", 1, alice(), None),
            on_call("PMODERA", 1, bob(), None),
            on_call("PUNUSED", 1, carol(), None),
        ],
        &ChosenPolicies::from("PUNUSED, PMODERA, PGONE01, PFISHNE"),
    );

    let ids: Vec<&str> = roster
        .policies
        .iter()
        .map(|policy| policy.id.as_str())
        .collect();
    assert_eq!(ids, ["PFISHNE", "PMODERA"]);
    assert_eq!(
        shifts_of(&roster, 1, 0),
        [Shift {
            user: bob(),
            ends: None
        }]
    );
}

#[test]
fn policies_are_sorted_by_name_then_by_id() {
    let roster = roster_of(
        vec![
            escalation_policy("PZZZZZ9", "Moderation", &[]),
            escalation_policy("PBBBBB2", "Lichess", &[]),
            escalation_policy("PAAAAA1", "Moderation", &[]),
        ],
        vec![],
    );

    let ids: Vec<&str> = roster
        .policies
        .iter()
        .map(|policy| policy.id.as_str())
        .collect();
    assert_eq!(ids, ["PBBBBB2", "PAAAAA1", "PZZZZZ9"]);
}

#[test]
fn levels_follow_the_rules_in_order_and_are_numbered_from_one() {
    let roster = roster_of(
        vec![escalation_policy(
            "PLICHES",
            "Lichess",
            &[
                &[Target::Schedule(primary())],
                &[],
                &[
                    Target::User(carol()),
                    Target::Schedule(backup()),
                    Target::User(alice()),
                    Target::Schedule(primary()),
                ],
            ],
        )],
        vec![],
    );

    assert_eq!(
        roster.policies[0].levels,
        [
            Level {
                number: EscalationLevel::new(1),
                schedules: vec![primary()],
                people: vec![],
                shifts: vec![],
            },
            Level {
                number: EscalationLevel::new(2),
                schedules: vec![],
                people: vec![],
                shifts: vec![],
            },
            Level {
                number: EscalationLevel::new(3),
                schedules: vec![backup(), primary()],
                people: vec![carol(), alice()],
                shifts: vec![],
            },
        ]
    );
}

#[test]
fn the_shifts_of_a_level_are_the_people_on_call_at_that_level_of_that_policy_sorted_by_user_id() {
    let roster = roster_of(
        vec![
            escalation_policy("PLICHES", "Lichess", &[&[], &[]]),
            escalation_policy("PMODERA", "Moderation", &[&[]]),
        ],
        vec![
            on_call("PLICHES", 1, carol(), Some(EVENING)),
            on_call("PMODERA", 1, bob(), None),
            on_call("PLICHES", 2, bob(), Some(MIDNIGHT)),
            on_call("PLICHES", 1, alice(), None),
        ],
    );

    assert_eq!(
        shifts_of(&roster, 0, 0),
        [
            Shift {
                user: alice(),
                ends: None
            },
            Shift {
                user: carol(),
                ends: Some(EVENING)
            },
        ]
    );
    assert_eq!(
        shifts_of(&roster, 0, 1),
        [Shift {
            user: bob(),
            ends: Some(MIDNIGHT)
        }]
    );
    assert_eq!(
        shifts_of(&roster, 1, 0),
        [Shift {
            user: bob(),
            ends: None
        }]
    );
}

#[test]
fn a_person_listed_twice_at_a_level_appears_once_with_the_later_end() {
    for ends in [[EVENING, MIDNIGHT], [MIDNIGHT, EVENING]] {
        let roster = roster_of(
            vec![escalation_policy("PLICHES", "Lichess", &[&[]])],
            ends.into_iter()
                .map(|end| on_call("PLICHES", 1, alice(), Some(end)))
                .collect(),
        );

        assert_eq!(
            shifts_of(&roster, 0, 0),
            [Shift {
                user: alice(),
                ends: Some(MIDNIGHT)
            }]
        );
    }
}

#[test]
fn no_end_is_later_than_any_end() {
    for ends in [[None, Some(MIDNIGHT)], [Some(MIDNIGHT), None]] {
        let roster = roster_of(
            vec![escalation_policy("PLICHES", "Lichess", &[&[]])],
            ends.into_iter()
                .map(|end| on_call("PLICHES", 1, alice(), end))
                .collect(),
        );

        assert_eq!(
            shifts_of(&roster, 0, 0),
            [Shift {
                user: alice(),
                ends: None
            }]
        );
    }
}

#[test]
fn someone_on_call_at_a_level_the_policy_has_no_rule_for_is_left_out() {
    let roster = roster_of(
        vec![escalation_policy("PLICHES", "Lichess", &[&[]])],
        vec![
            on_call("PLICHES", 2, alice(), None),
            on_call("PGONE01", 1, bob(), None),
        ],
    );

    assert_eq!(roster.policies.len(), 1);
    assert_eq!(roster.policies[0].levels.len(), 1);
    assert_eq!(shifts_of(&roster, 0, 0), []);
}
