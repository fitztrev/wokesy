use super::*;
use crate::people::Email;

fn user(id: &str, name: &str, email: &str) -> User {
    User {
        id: UserId::try_new(id).unwrap(),
        name: Name::try_new(name).unwrap(),
        email: Email::try_new(email).unwrap(),
    }
}

fn member(id: u64, name: &str, emails: &[&str]) -> ChatMember {
    ChatMember {
        id: ChatUserId(id),
        name: Name::try_new(name).unwrap(),
        emails: emails
            .iter()
            .map(|email| Email::try_new(*email).unwrap())
            .collect(),
    }
}

fn links(links: &str) -> Links {
    links.parse().unwrap()
}

fn alice() -> User {
    user("PALICE1", "Alice Example", "alice@example.com")
}

fn chat_user_of_alice(members: &[ChatMember], links: &Links) -> Option<ChatUserId> {
    let alice = alice();
    Directory::matching(std::slice::from_ref(&alice), members, links).chat_user_of(&alice.id)
}

#[test]
fn a_link_for_the_pagerduty_email_names_the_chat_user() {
    let members = [member(12, "Someone Else", &["someone@example.com"])];

    assert_eq!(
        chat_user_of_alice(&members, &links("alice@example.com=12")),
        Some(ChatUserId(12))
    );
}

#[test]
fn a_link_wins_even_when_no_member_has_that_id() {
    assert_eq!(
        chat_user_of_alice(&[], &links("alice@example.com=12")),
        Some(ChatUserId(12))
    );
}

#[test]
fn a_link_wins_over_a_member_with_the_same_email_and_name() {
    let members = [member(13, "Alice Example", &["alice@example.com"])];

    assert_eq!(
        chat_user_of_alice(&members, &links("alice@example.com=12")),
        Some(ChatUserId(12))
    );
}

#[test]
fn the_only_member_with_the_email_among_its_emails_is_the_chat_user() {
    let members = [
        member(12, "Al", &["user12@zulip.example.com", "alice@example.com"]),
        member(13, "Bob Example", &["bob@example.com"]),
    ];

    assert_eq!(
        chat_user_of_alice(&members, &Links::default()),
        Some(ChatUserId(12))
    );
}

#[test]
fn the_email_wins_over_the_name() {
    let members = [
        member(12, "Alice Example", &["other-alice@example.com"]),
        member(13, "Al", &["alice@example.com"]),
    ];

    assert_eq!(
        chat_user_of_alice(&members, &Links::default()),
        Some(ChatUserId(13))
    );
}

#[test]
fn two_members_sharing_the_email_match_nobody() {
    let members = [
        member(12, "Al", &["alice@example.com"]),
        member(13, "Alice Again", &["alice@example.com"]),
    ];

    assert_eq!(chat_user_of_alice(&members, &Links::default()), None);
}

#[test]
fn the_name_decides_between_members_sharing_the_email() {
    let members = [
        member(12, "Al", &["alice@example.com"]),
        member(13, "Alice Example", &["alice@example.com"]),
    ];

    assert_eq!(
        chat_user_of_alice(&members, &Links::default()),
        Some(ChatUserId(13))
    );
}

#[test]
fn the_only_member_with_the_same_name_in_any_case_is_the_chat_user() {
    let members = [
        member(12, "ALICE example", &["user12@zulip.example.com"]),
        member(13, "Alice Examples", &["user13@zulip.example.com"]),
    ];

    assert_eq!(
        chat_user_of_alice(&members, &Links::default()),
        Some(ChatUserId(12))
    );
}

#[test]
fn two_members_sharing_the_name_match_nobody() {
    let members = [
        member(12, "Alice Example", &["user12@zulip.example.com"]),
        member(13, "alice example", &["user13@zulip.example.com"]),
    ];

    assert_eq!(chat_user_of_alice(&members, &Links::default()), None);
}

#[test]
fn a_user_no_rule_matches_has_no_chat_user_and_keeps_its_name() {
    let alice = alice();
    let members = [member(13, "Bob Example", &["bob@example.com"])];

    let directory = Directory::matching(
        std::slice::from_ref(&alice),
        &members,
        &links("bob@example.com=13"),
    );

    assert_eq!(directory.chat_user_of(&alice.id), None);
    assert_eq!(directory.name_of(&alice.id), Some(&alice.name));
}

#[test]
fn a_user_pagerduty_did_not_list_has_no_chat_user_and_no_name() {
    let directory = Directory::matching(&[alice()], &[], &links("alice@example.com=12"));
    let stranger = UserId::try_new("PGONE01").unwrap();

    assert_eq!(directory.chat_user_of(&stranger), None);
    assert_eq!(directory.name_of(&stranger), None);
}

#[test]
fn a_chat_user_leads_back_to_the_pagerduty_user_matched_to_it() {
    let alice = alice();
    let bob = user("PBOB001", "Bob Example", "bob@example.com");
    let members = [
        member(12, "Al", &["alice@example.com"]),
        member(13, "Bob Example", &["user13@zulip.example.com"]),
        member(14, "Carol Example", &["carol@example.com"]),
    ];

    let directory = Directory::matching(&[alice.clone(), bob.clone()], &members, &Links::default());

    assert_eq!(directory.pagerduty_user_of(ChatUserId(12)), Some(&alice));
    assert_eq!(directory.pagerduty_user_of(ChatUserId(13)), Some(&bob));
    assert_eq!(directory.pagerduty_user_of(ChatUserId(14)), None);
    assert_eq!(directory.pagerduty_user_of(ChatUserId(15)), None);
}

#[test]
fn a_member_matched_by_a_link_or_by_email_is_not_matched_to_another_pagerduty_user_by_name() {
    let linked = user("PZZZZZ9", "Alice Example", "alice@example.com");
    let by_email = user("PMMMMM5", "Alice Example", "alice@lichess.example.com");
    let by_name = user("PAAAAA1", "Alice Example", "alice@old.example.com");
    let members = [member(12, "Alice Example", &["alice@lichess.example.com"])];

    let everyone = Directory::matching(
        &[by_name.clone(), by_email.clone(), linked.clone()],
        &members,
        &links("alice@example.com=12"),
    );
    let without_a_link = Directory::matching(
        &[by_name.clone(), by_email.clone()],
        &members,
        &Links::default(),
    );
    let alone = Directory::matching(std::slice::from_ref(&by_name), &members, &Links::default());

    assert_eq!(everyone.chat_user_of(&linked.id), Some(ChatUserId(12)));
    assert_eq!(everyone.chat_user_of(&by_email.id), Some(ChatUserId(12)));
    assert_eq!(everyone.chat_user_of(&by_name.id), None);
    assert_eq!(everyone.pagerduty_user_of(ChatUserId(12)), Some(&linked));
    assert_eq!(
        without_a_link.chat_user_of(&by_email.id),
        Some(ChatUserId(12))
    );
    assert_eq!(without_a_link.chat_user_of(&by_name.id), None);
    assert_eq!(
        without_a_link.pagerduty_user_of(ChatUserId(12)),
        Some(&by_email)
    );
    assert_eq!(alone.chat_user_of(&by_name.id), Some(ChatUserId(12)));
}

#[test]
fn a_member_linked_to_one_pagerduty_user_is_not_matched_to_another_who_has_its_name() {
    let linked = user("PALICE1", "Alice Example", "alice@example.com");
    let namesake = user("PALICE2", "Al", "al@example.com");
    let members = [member(12, "Al", &["user12@zulip.example.com"])];

    let directory = Directory::matching(
        &[namesake.clone(), linked.clone()],
        &members,
        &links("alice@example.com=12"),
    );

    assert_eq!(directory.chat_user_of(&namesake.id), None);
    assert_eq!(directory.pagerduty_user_of(ChatUserId(12)), Some(&linked));
}

#[test]
fn a_claimed_member_sharing_the_name_with_another_still_leaves_the_name_ambiguous() {
    let by_email = user("PALICE1", "Alice Example", "alice@example.com");
    let by_name = user("PALICE2", "Alice Example", "alice@old.example.com");
    let members = [
        member(12, "Alice Example", &["alice@example.com"]),
        member(13, "Alice Example", &["user13@zulip.example.com"]),
    ];

    let directory = Directory::matching(
        &[by_email.clone(), by_name.clone()],
        &members,
        &Links::default(),
    );

    assert_eq!(directory.chat_user_of(&by_email.id), Some(ChatUserId(12)));
    assert_eq!(directory.chat_user_of(&by_name.id), None);
}

#[test]
fn a_chat_user_matched_to_two_pagerduty_users_by_the_same_rule_leads_back_to_neither() {
    let work = user("PALICE1", "Alice Example", "alice@example.com");
    let home = user("PALICE2", "Alice Example", "alice@home.example.com");

    let directory = Directory::matching(
        &[work.clone(), home.clone()],
        &[],
        &links("alice@example.com=12, alice@home.example.com=12"),
    );

    assert_eq!(directory.chat_user_of(&work.id), Some(ChatUserId(12)));
    assert_eq!(directory.chat_user_of(&home.id), Some(ChatUserId(12)));
    assert_eq!(directory.pagerduty_user_of(ChatUserId(12)), None);
}

#[test]
fn directories_matched_from_the_same_people_in_another_order_are_equal() {
    let alice = alice();
    let bob = user("PBOB001", "Bob Example", "bob@example.com");
    let al = member(12, "Al", &["alice@example.com"]);
    let robert = member(13, "Bob Example", &["user13@zulip.example.com"]);

    assert_eq!(
        Directory::matching(
            &[alice.clone(), bob.clone()],
            &[al.clone(), robert.clone()],
            &Links::default()
        ),
        Directory::matching(&[bob, alice], &[robert, al], &Links::default())
    );
}
