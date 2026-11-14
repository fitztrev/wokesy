use super::*;

fn email(address: &str) -> Email {
    Email::try_new(address).unwrap()
}

#[test]
fn links_are_read_with_or_without_whitespace_around_them() {
    let spaced: Links = " alice@example.com = 12 , bob@example.com=34 "
        .parse()
        .unwrap();
    let dense: Links = "alice@example.com=12,bob@example.com=34".parse().unwrap();
    assert_eq!(spaced, dense);
    assert_eq!(
        dense.chat_user_of(&email("alice@example.com")),
        Some(ChatUserId(12))
    );
    assert_eq!(
        dense.chat_user_of(&email("bob@example.com")),
        Some(ChatUserId(34))
    );
}

#[test]
fn a_link_is_found_whatever_the_case_of_the_email_it_was_written_with() {
    let links: Links = "Alice@Example.com=12".parse().unwrap();
    assert_eq!(
        links.chat_user_of(&email("alice@example.com")),
        Some(ChatUserId(12))
    );
}

#[test]
fn an_email_without_a_link_has_no_chat_user() {
    let links: Links = "alice@example.com=12".parse().unwrap();
    assert_eq!(links.chat_user_of(&email("bob@example.com")), None);
}

#[test]
fn a_blank_string_and_a_trailing_comma_add_no_links() {
    assert_eq!("".parse::<Links>().unwrap(), Links::default());
    assert_eq!("  ".parse::<Links>().unwrap(), Links::default());
    assert_eq!(
        "alice@example.com=12,".parse::<Links>().unwrap(),
        "alice@example.com=12".parse::<Links>().unwrap()
    );
}

#[test]
fn a_malformed_link_is_refused_and_named_by_its_position() {
    for (links, position) in [
        ("alice@example.com", 1),
        ("alice@example.com=12, bob@example.com", 2),
        ("alice@example.com=12, bob=34", 2),
        ("alice@example.com=12, =34", 2),
        ("alice@example.com=twelve", 1),
        ("alice@example.com=", 1),
        ("alice@example.com=-12", 1),
        ("alice@example.com=12 bob@example.com=34", 1),
    ] {
        assert_eq!(
            links.parse::<Links>(),
            Err(MalformedLink { position }),
            "{links}"
        );
    }
}

#[test]
fn the_error_does_not_repeat_the_link_because_links_are_secret() {
    let error = "alice@example.com".parse::<Links>().unwrap_err();
    assert_eq!(
        error.to_string(),
        "link 1 is not an `email=zulip_user_id` pair"
    );
}
