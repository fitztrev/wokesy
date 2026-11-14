use time::macros::datetime;

use super::*;
use crate::on_call::changes::changes;
use crate::on_call::testing::{directory, level, policy, roster, shift, user};

fn alice() -> UserRef {
    user("PALICE1", "Alice Example")
}

fn bob() -> UserRef {
    user("PBOB001", "Bob Example")
}

fn carol() -> UserRef {
    user("PCAROL1", "Carol Example")
}

fn alice_and_bob_in_chat() -> Directory {
    directory(&[(&alice(), 12), (&bob(), 13)])
}

fn until_the_evening(user: UserRef) -> Shift {
    shift(user, Some(datetime!(2026-11-14 21:00 +1)))
}

fn open_ended(user: UserRef) -> Shift {
    shift(user, None)
}

fn lichess(first_line: &[Shift]) -> Roster {
    roster(&[policy("PLICHES", "Lichess", &[level(1, first_line)])])
}

fn changed(announced: &Roster, latest: &Roster) -> String {
    on_call_changed(&changes(announced, latest), &alice_and_bob_in_chat())
}

#[test]
fn on_call_now_has_a_line_for_each_level_of_each_policy() {
    let roster = roster(&[
        policy(
            "PLICHES",
            "Lichess",
            &[
                level(1, &[until_the_evening(alice())]),
                level(2, &[open_ended(bob()), open_ended(carol())]),
            ],
        ),
        policy(
            "PMODERA",
            "Moderation",
            &[level(1, &[until_the_evening(carol())])],
        ),
    ]);

    assert_eq!(
        on_call_now(&roster, &alice_and_bob_in_chat()),
        "\
:pager: **On call now**
- **Lichess**, level 1: @_**|12** until <time:2026-11-14T20:00:00Z>
- **Lichess**, level 2: @_**|13**, Carol Example
- **Moderation**, level 1: Carol Example until <time:2026-11-14T20:00:00Z>"
    );
}

#[test]
fn a_level_with_nobody_on_call_is_a_warning() {
    let roster = roster(&[policy(
        "PLICHES",
        "Lichess",
        &[level(1, &[open_ended(alice())]), level(2, &[])],
    )]);

    assert_eq!(
        on_call_now(&roster, &alice_and_bob_in_chat()),
        "\
:pager: **On call now**
- **Lichess**, level 1: @_**|12**
- **Lichess**, level 2: :warning: nobody"
    );
}

#[test]
fn without_a_policy_in_use_nobody_is_on_call() {
    assert_eq!(
        on_call_now(&roster(&[]), &alice_and_bob_in_chat()),
        "\
:pager: **On call now**
No escalation policy is used by a service, so nobody is on call."
    );
}

#[test]
fn nobody_is_named_with_a_mention_that_notifies_them() {
    let everyone = [
        until_the_evening(alice()),
        open_ended(bob()),
        open_ended(carol()),
    ];
    let now = on_call_now(&lichess(&everyone), &alice_and_bob_in_chat());
    let change = changed(&lichess(&everyone), &lichess(&[]));

    assert!(!now.contains("@**"), "{now}");
    assert!(!change.contains("@**"), "{change}");
}

#[test]
fn a_change_names_who_is_on_call_at_the_level_and_who_was() {
    let announced = lichess(&[until_the_evening(bob())]);
    let latest = lichess(&[until_the_evening(alice())]);

    assert_eq!(
        changed(&announced, &latest),
        "\
:pager: **On call changed**
- **Lichess**, level 1: @_**|12** until <time:2026-11-14T20:00:00Z> (was @_**|13**)"
    );
}

#[test]
fn a_change_lists_everyone_who_was_on_call_without_their_shift_ends() {
    let announced = lichess(&[
        until_the_evening(alice()),
        until_the_evening(bob()),
        until_the_evening(carol()),
    ]);
    let latest = lichess(&[open_ended(carol())]);

    assert_eq!(
        changed(&announced, &latest),
        "\
:pager: **On call changed**
- **Lichess**, level 1: Carol Example (was @_**|12**, @_**|13**, Carol Example)"
    );
}

#[test]
fn a_change_from_nobody_and_a_change_to_nobody_say_so() {
    let announced = roster(&[policy(
        "PLICHES",
        "Lichess",
        &[level(1, &[]), level(2, &[open_ended(bob())])],
    )]);
    let latest = roster(&[policy(
        "PLICHES",
        "Lichess",
        &[level(1, &[open_ended(alice())]), level(2, &[])],
    )]);

    assert_eq!(
        changed(&announced, &latest),
        "\
:pager: **On call changed**
- **Lichess**, level 1: @_**|12** (was nobody)
- **Lichess**, level 2: :warning: nobody (was @_**|13**)"
    );
}
