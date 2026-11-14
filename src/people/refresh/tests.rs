use std::time::Duration;

use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::chat::ChatUserId;
use crate::pagerduty::UserId;
use crate::zulip::{self, BotEmail, InboxConfig, ZulipMembers};

const PATIENCE: Duration = Duration::from_secs(5);
const RARELY: Duration = Duration::from_hours(1);
const OFTEN: Duration = Duration::from_millis(20);
const PAGERDUTY_USERS: &str = "/users";
const ZULIP_MEMBERS: &str = "/api/v1/users";

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn users(users: &[(&str, &str, &str)]) -> ResponseTemplate {
    let users: Vec<Value> = users
        .iter()
        .map(|(id, name, email)| json!({ "id": id, "name": name, "email": email }))
        .collect();
    ok(json!({ "users": users, "more": false }))
}

fn members(members: &[(u64, &str, &str)]) -> ResponseTemplate {
    let members: Vec<Value> = members
        .iter()
        .map(|(id, name, email)| {
            json!({
                "user_id": id,
                "full_name": name,
                "email": email,
                "is_active": true,
                "is_bot": false,
            })
        })
        .collect();
    ok(json!({ "result": "success", "msg": "", "members": members }))
}

fn alice_and_bob() -> ResponseTemplate {
    users(&[
        ("PALICE1", "Alice Example", "alice@example.com"),
        ("PBOB001", "Bob Example", "bob@pagerduty.example.com"),
    ])
}

fn only_alice_in_chat() -> ResponseTemplate {
    members(&[(12, "Al", "alice@example.com")])
}

fn user_id(id: &str) -> UserId {
    UserId::try_new(id).unwrap()
}

async fn answers_in_turn(server: &MockServer, at: &str, answers: &[ResponseTemplate]) {
    for answer in answers {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(answer.clone())
            .up_to_n_times(1)
            .mount(server)
            .await;
    }
    if let Some(last) = answers.last() {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(last.clone())
            .mount(server)
            .await;
    }
}

async fn requests_reach(server: &MockServer, count: usize) {
    let reached = async {
        while server.received_requests().await.unwrap().len() < count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    tokio::time::timeout(PATIENCE, reached).await.unwrap();
}

struct Matching {
    pagerduty: MockServer,
    zulip: MockServer,
    task: JoinHandle<()>,
    directory: watch::Receiver<Option<Directory>>,
}

impl Matching {
    async fn start(
        pagerduty_answers: &[ResponseTemplate],
        zulip_answers: &[ResponseTemplate],
        links: &str,
        refresh_interval: Duration,
    ) -> Self {
        let pagerduty = MockServer::start().await;
        answers_in_turn(&pagerduty, PAGERDUTY_USERS, pagerduty_answers).await;
        let zulip = MockServer::start().await;
        answers_in_turn(&zulip, ZULIP_MEMBERS, zulip_answers).await;
        let http = reqwest::Client::new();
        let api = pagerduty::Api::new(
            http.clone(),
            &pagerduty::Config {
                token: SecretString::from("u+secret"),
                base_url: pagerduty.uri().parse().unwrap(),
                events: None,
            },
        );
        let chat = ZulipMembers::new(
            http,
            &zulip::Config {
                site: zulip.uri().parse().unwrap(),
                email: BotEmail::try_new("wokesy-bot@zulip.example.com").unwrap(),
                api_key: SecretString::from("key"),
                inbox: InboxConfig::default(),
            },
        );
        let config = Config {
            links: links.parse().unwrap(),
            refresh_interval,
        };
        let (published, directory) = watch::channel(None);
        let task = tokio::spawn(keep_matched(api, chat, config, published));
        Self {
            pagerduty,
            zulip,
            task,
            directory,
        }
    }

    async fn next_directory(&mut self) -> Directory {
        tokio::time::timeout(PATIENCE, self.directory.changed())
            .await
            .unwrap()
            .unwrap();
        self.directory.borrow_and_update().clone().unwrap()
    }
}

impl Drop for Matching {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn the_first_directory_matches_the_pagerduty_users_with_the_chat_members_and_the_links() {
    let mut matching = Matching::start(
        &[alice_and_bob()],
        &[only_alice_in_chat()],
        "bob@pagerduty.example.com=34",
        RARELY,
    )
    .await;

    let directory = matching.next_directory().await;

    assert_eq!(
        directory.chat_user_of(&user_id("PALICE1")),
        Some(ChatUserId(12))
    );
    assert_eq!(
        directory.chat_user_of(&user_id("PBOB001")),
        Some(ChatUserId(34))
    );
}

#[tokio::test]
async fn people_who_changed_are_matched_again_at_a_later_refresh() {
    let mut matching = Matching::start(
        &[alice_and_bob()],
        &[
            only_alice_in_chat(),
            members(&[
                (12, "Al", "alice@example.com"),
                (13, "Bob Example", "bob@example.com"),
            ]),
        ],
        "",
        OFTEN,
    )
    .await;

    let first = matching.next_directory().await;
    let second = matching.next_directory().await;

    assert_eq!(first.chat_user_of(&user_id("PBOB001")), None);
    assert_eq!(
        second.chat_user_of(&user_id("PBOB001")),
        Some(ChatUserId(13))
    );
}

#[tokio::test]
async fn the_same_people_are_not_published_again() {
    let mut matching =
        Matching::start(&[alice_and_bob()], &[only_alice_in_chat()], "", OFTEN).await;

    matching.next_directory().await;
    requests_reach(&matching.pagerduty, 4).await;

    assert!(!matching.directory.has_changed().unwrap());
}

#[tokio::test]
async fn the_previous_directory_is_kept_when_pagerduty_cannot_be_read() {
    let mut matching = Matching::start(
        &[alice_and_bob(), ResponseTemplate::new(502)],
        &[only_alice_in_chat()],
        "",
        OFTEN,
    )
    .await;

    let first = matching.next_directory().await;
    requests_reach(&matching.pagerduty, 4).await;

    assert!(!matching.directory.has_changed().unwrap());
    assert_eq!(*matching.directory.borrow(), Some(first));
    assert!(!matching.task.is_finished());
}

#[tokio::test]
async fn the_previous_directory_is_kept_when_the_chat_members_cannot_be_read() {
    let refusal = ResponseTemplate::new(401).set_body_json(json!({
        "result": "error",
        "msg": "Invalid API key",
        "code": "UNAUTHORIZED",
    }));
    let mut matching = Matching::start(
        &[
            alice_and_bob(),
            users(&[("PCAROL1", "Carol Example", "carol@example.com")]),
        ],
        &[only_alice_in_chat(), refusal],
        "",
        OFTEN,
    )
    .await;

    let first = matching.next_directory().await;
    requests_reach(&matching.zulip, 4).await;

    assert!(!matching.directory.has_changed().unwrap());
    assert_eq!(*matching.directory.borrow(), Some(first));
    assert!(!matching.task.is_finished());
}

#[tokio::test]
async fn a_directory_is_published_once_a_read_that_failed_at_first_works() {
    let mut matching = Matching::start(
        &[ResponseTemplate::new(502), alice_and_bob()],
        &[only_alice_in_chat()],
        "",
        OFTEN,
    )
    .await;

    let directory = matching.next_directory().await;

    assert_eq!(
        directory.chat_user_of(&user_id("PALICE1")),
        Some(ChatUserId(12))
    );
}

#[tokio::test]
async fn matching_ends_once_nobody_reads_the_directory() {
    let mut matching =
        Matching::start(&[alice_and_bob()], &[only_alice_in_chat()], "", RARELY).await;
    matching.next_directory().await;

    matching.directory = watch::channel(None).1;

    assert!(
        tokio::time::timeout(PATIENCE, &mut matching.task)
            .await
            .is_ok()
    );
}
