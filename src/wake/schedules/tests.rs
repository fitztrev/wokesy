use super::*;

fn ids(schedules: &Schedules) -> Vec<&str> {
    schedules.ids().iter().map(|id| id.as_str()).collect()
}

#[test]
fn schedules_are_read_in_order_with_or_without_whitespace_around_them() {
    let spaced = Schedules::from(" PI7DH85 , PFISHNT ");
    let dense = Schedules::from("PI7DH85,PFISHNT");

    assert_eq!(spaced, dense);
    assert_eq!(ids(&dense), ["PI7DH85", "PFISHNT"]);
}

#[test]
fn a_blank_string_and_blank_pieces_name_no_schedule() {
    assert_eq!(Schedules::from(""), Schedules::default());
    assert_eq!(Schedules::from("  "), Schedules::default());
    assert_eq!(
        Schedules::from("PI7DH85,, ,PFISHNT,"),
        Schedules::from("PI7DH85,PFISHNT")
    );
}

#[test]
fn a_schedule_named_twice_is_read_once() {
    let schedules = Schedules::from("PI7DH85, PFISHNT, PI7DH85");

    assert_eq!(ids(&schedules), ["PI7DH85", "PFISHNT"]);
}
