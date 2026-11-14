use secrecy::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::chat::ChatUserId;
use crate::zulip::{ApiError, BotEmail, InboxConfig};

async fn zulip_answering(status: u16, body: Value) -> (MockServer, ZulipMembers) {
    let zulip = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&zulip)
        .await;
    let config = Config {
        site: zulip.uri().parse().unwrap(),
        email: BotEmail::try_new("wokesy-bot@zulip.example.com").unwrap(),
        api_key: SecretString::from("key"),
        inbox: InboxConfig::default(),
    };
    let members = ZulipMembers::new(reqwest::Client::new(), &config);
    (zulip, members)
}

async fn read_from(members: Value) -> Vec<ChatMember> {
    let body = json!({ "result": "success", "msg": "", "members": members });
    let (_zulip, members) = zulip_answering(200, body).await;
    members.members().await.unwrap()
}

fn person(user_id: u64, full_name: &str, email: &str) -> Value {
    json!({
        "user_id": user_id,
        "full_name": full_name,
        "email": email,
        "is_active": true,
        "is_bot": false,
    })
}

fn with(mut member: Value, field: &str, value: Value) -> Value {
    member[field] = value;
    member
}

fn chat_member(id: u64, name: &str, emails: &[&str]) -> ChatMember {
    ChatMember {
        id: ChatUserId(id),
        name: Name::try_new(name).unwrap(),
        emails: emails
            .iter()
            .map(|email| Email::try_new(*email).unwrap())
            .collect(),
    }
}

#[tokio::test]
async fn only_active_people_are_members() {
    let members = read_from(json!([
        person(12, "Alice Example", "alice@example.com"),
        with(
            person(13, "Bob Gone", "bob@example.com"),
            "is_active",
            json!(false)
        ),
        with(
            person(14, "Wokesy", "wokesy-bot@zulip.example.com"),
            "is_bot",
            json!(true)
        ),
        person(15, "Carol Example", "carol@example.com"),
    ]))
    .await;

    assert_eq!(
        members,
        [
            chat_member(12, "Alice Example", &["alice@example.com"]),
            chat_member(15, "Carol Example", &["carol@example.com"]),
        ]
    );
}

#[tokio::test]
async fn the_delivery_email_comes_before_the_email_zulip_shows() {
    let members = read_from(json!([with(
        person(12, "Alice Example", "user12@zulip.example.com"),
        "delivery_email",
        json!("alice@example.com")
    )]))
    .await;

    assert_eq!(
        members,
        [chat_member(
            12,
            "Alice Example",
            &["alice@example.com", "user12@zulip.example.com"]
        )]
    );
}

#[tokio::test]
async fn a_hidden_delivery_email_leaves_the_email_zulip_shows() {
    let members = read_from(json!([with(
        person(12, "Alice Example", "user12@zulip.example.com"),
        "delivery_email",
        Value::Null
    )]))
    .await;

    assert_eq!(
        members,
        [chat_member(
            12,
            "Alice Example",
            &["user12@zulip.example.com"]
        )]
    );
}

#[tokio::test]
async fn an_email_that_is_also_the_delivery_email_is_listed_once() {
    let members = read_from(json!([with(
        person(12, "Alice Example", "alice@example.com"),
        "delivery_email",
        json!("Alice@Example.com")
    )]))
    .await;

    assert_eq!(
        members,
        [chat_member(12, "Alice Example", &["alice@example.com"])]
    );
}

#[tokio::test]
async fn a_member_without_a_usable_name_or_email_is_skipped_and_the_others_are_kept() {
    let members = read_from(json!([
        person(12, "  ", "nameless@example.com"),
        person(13, "Bob Example", "not an email"),
        with(
            person(14, "Carol Example", "carol@example.com"),
            "delivery_email",
            json!("")
        ),
        person(15, "Dave Example", "dave@example.com"),
    ]))
    .await;

    assert_eq!(
        members,
        [chat_member(15, "Dave Example", &["dave@example.com"])]
    );
}

#[tokio::test]
async fn a_refusal_from_zulip_is_a_failed_read_carrying_its_reason() {
    let refusal = json!({
        "result": "error",
        "msg": "Invalid API key",
        "code": "UNAUTHORIZED",
    });
    let (_zulip, members) = zulip_answering(401, refusal).await;

    let MembersError(cause) = members.members().await.unwrap_err();

    assert!(matches!(
        cause.downcast_ref::<ApiError>(),
        Some(ApiError::Rejected { reason, .. }) if reason == "Invalid API key"
    ));
}
