use super::*;

fn names(channels: &Channels) -> Vec<&str> {
    channels.names().iter().map(|name| name.as_str()).collect()
}

#[test]
fn channels_are_read_in_order_with_or_without_whitespace_around_them() {
    let spaced = Channels::from(" sysadmin , on-call ");
    let dense = Channels::from("sysadmin,on-call");
    assert_eq!(spaced, dense);
    assert_eq!(names(&dense), ["sysadmin", "on-call"]);
}

#[test]
fn a_channel_name_keeps_its_case_and_the_spaces_inside_it() {
    let channels = Channels::from("Sysadmin team, on call");
    assert_eq!(names(&channels), ["Sysadmin team", "on call"]);
}

#[test]
fn a_blank_string_and_blank_pieces_add_no_channels() {
    assert_eq!(Channels::from(""), Channels::default());
    assert_eq!(Channels::from("  "), Channels::default());
    assert_eq!(
        Channels::from("sysadmin,, ,on-call,"),
        Channels::from("sysadmin,on-call")
    );
}
