use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::*;
use crate::chat::{ChannelName, ChatUserId, MessageId, TopicName};
use crate::people::Name;
use crate::testing::Logs;
use crate::zulip::{BotEmail, Config};

const BOT: u64 = 99;
const ALICE: u64 = 10;
const BOB: u64 = 11;

type Form = BTreeMap<String, String>;
type Reply = (StatusCode, Value);

#[derive(Clone)]
struct FakeState {
    replies: Arc<Mutex<mpsc::UnboundedReceiver<Reply>>>,
    registrations: mpsc::UnboundedSender<Form>,
    registered: Arc<AtomicU32>,
    polls: mpsc::UnboundedSender<(String, String)>,
}

fn success(fields: Value) -> Value {
    let mut reply = json!({ "result": "success", "msg": "" });
    if let (Value::Object(reply), Value::Object(fields)) = (&mut reply, fields) {
        reply.extend(fields);
    }
    reply
}

async fn own_user() -> Json<Value> {
    Json(success(json!({
        "user_id": BOT,
        "full_name": "Wokesy",
        "email": "wokesy-bot@zulip.example.com",
    })))
}

async fn register(State(state): State<FakeState>, body: String) -> Json<Value> {
    let number = state.registered.fetch_add(1, Ordering::Relaxed) + 1;
    let registration = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    state.registrations.send(registration).unwrap();
    Json(success(json!({
        "queue_id": format!("queue-{number}"),
        "last_event_id": -1,
    })))
}

async fn events(
    State(state): State<FakeState>,
    Query(query): Query<BTreeMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    state
        .polls
        .send((query["queue_id"].clone(), query["last_event_id"].clone()))
        .unwrap();
    let reply = state.replies.lock().await.recv().await;
    match reply {
        Some((status, body)) => (status, Json(body)),
        None => std::future::pending().await,
    }
}

struct FakeZulip {
    reading: JoinHandle<()>,
    replies: mpsc::UnboundedSender<Reply>,
    registrations: mpsc::UnboundedReceiver<Form>,
    polls: mpsc::UnboundedReceiver<(String, String)>,
}

impl FakeZulip {
    async fn start() -> (Self, mpsc::Receiver<IncomingMessage>) {
        let inbox = InboxConfig {
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(50),
        };
        Self::start_with(inbox).await
    }

    async fn start_with(inbox: InboxConfig) -> (Self, mpsc::Receiver<IncomingMessage>) {
        let (replies, replies_out) = mpsc::unbounded_channel();
        let (registrations_in, registrations) = mpsc::unbounded_channel();
        let (polls_in, polls) = mpsc::unbounded_channel();
        let state = FakeState {
            replies: Arc::new(Mutex::new(replies_out)),
            registrations: registrations_in,
            registered: Arc::default(),
            polls: polls_in,
        };
        let router = Router::new()
            .route("/api/v1/users/me", get(own_user))
            .route("/api/v1/register", post(register))
            .route("/api/v1/events", get(events))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = Config {
            site: format!("http://{address}").parse().unwrap(),
            email: BotEmail::try_new("wokesy-bot@zulip.example.com").unwrap(),
            api_key: SecretString::from("key"),
            inbox,
        };
        let api = Api::new(reqwest::Client::new(), &config);
        let (incoming, received) = mpsc::channel(16);
        let reading = tokio::spawn(keep_reading(api, config.inbox, incoming));
        let zulip = Self {
            reading,
            replies,
            registrations,
            polls,
        };
        (zulip, received)
    }

    fn reply(&self, status: StatusCode, body: Value) {
        self.replies.send((status, body)).unwrap();
    }

    fn expire(&self, queue: &str) {
        self.reply(
            StatusCode::BAD_REQUEST,
            json!({
                "result": "error",
                "code": "BAD_EVENT_QUEUE_ID",
                "msg": format!("Bad event queue ID: {queue}"),
                "queue_id": queue,
            }),
        );
    }

    fn deliver(&self, events: Value) {
        let mut body = success(json!({ "queue_id": "ignored" }));
        body["events"] = events;
        self.reply(StatusCode::OK, body);
    }

    async fn next_poll(&mut self) -> (String, String) {
        within_a_while(self.polls.recv()).await
    }
}

async fn within_a_while<T>(next: impl Future<Output = Option<T>>) -> T {
    tokio::time::timeout(Duration::from_secs(5), next)
        .await
        .unwrap()
        .unwrap()
}

fn full_name_of(user_id: u64) -> &'static str {
    match user_id {
        BOT => "Wokesy",
        ALICE => "Alice Example",
        _ => "Bob Example",
    }
}

fn direct_message(id: i64, sender: u64, content: &str) -> Value {
    group_message(id, sender, &[sender, BOT], content)
}

fn group_message(id: i64, sender: u64, recipients: &[u64], content: &str) -> Value {
    let recipients: Vec<Value> = recipients.iter().map(|id| json!({ "id": id })).collect();
    json!({
        "id": id,
        "type": "message",
        "flags": ["read"],
        "message": {
            "id": 1000 + id,
            "sender_id": sender,
            "sender_full_name": full_name_of(sender),
            "content": content,
            "type": "private",
            "display_recipient": recipients,
            "subject": "",
        },
    })
}

fn channel_message(id: i64, sender: u64, content: &str, flags: &[&str]) -> Value {
    json!({
        "id": id,
        "type": "message",
        "flags": flags,
        "message": {
            "id": 1000 + id,
            "sender_id": sender,
            "sender_full_name": full_name_of(sender),
            "content": content,
            "type": "stream",
            "display_recipient": "sysadmin",
            "stream_id": 1,
            "subject": "outage",
        },
    })
}

fn received_from(
    event_id: u64,
    sender: u64,
    reply_to: Conversation,
    text: &str,
) -> IncomingMessage {
    IncomingMessage {
        id: MessageId(1000 + event_id),
        sender: ChatUserId(sender),
        sender_name: Name::try_new(full_name_of(sender)).unwrap(),
        reply_to,
        text: CommandText::new(text),
    }
}

fn outage_topic() -> Conversation {
    Conversation::Topic {
        channel: ChannelName::try_new("sysadmin").unwrap(),
        topic: TopicName::try_new("outage").unwrap(),
    }
}

fn first_poll_of(queue: &str) -> (String, String) {
    (queue.to_owned(), "-1".to_owned())
}

#[tokio::test]
async fn the_queue_is_registered_for_messages_only_in_raw_markdown_from_every_public_channel() {
    let (mut zulip, _received) = FakeZulip::start().await;

    let registration = within_a_while(zulip.registrations.recv()).await;

    assert_eq!(registration["event_types"], r#"["message"]"#);
    assert_eq!(registration["apply_markdown"], "false");
    assert_eq!(registration["all_public_streams"], "true");
}

#[tokio::test]
async fn a_direct_message_is_received_with_its_sender_and_answered_directly() {
    let (zulip, mut received) = FakeZulip::start().await;

    zulip.deliver(json!([direct_message(1, ALICE, " wake me for 4 hours ")]));

    assert_eq!(
        within_a_while(received.recv()).await,
        received_from(
            1,
            ALICE,
            Conversation::Direct {
                user_id: ChatUserId(ALICE)
            },
            "wake me for 4 hours"
        )
    );
}

#[tokio::test]
async fn a_channel_message_that_mentions_the_bot_is_received_without_the_mention_and_answered_in_its_topic()
 {
    let (zulip, mut received) = FakeZulip::start().await;

    zulip.deliver(json!([channel_message(
        1,
        BOB,
        "@**Wokesy** page lichess.org is down",
        &["mentioned"]
    )]));

    assert_eq!(
        within_a_while(received.recv()).await,
        received_from(1, BOB, outage_topic(), "page lichess.org is down")
    );
}

#[tokio::test]
async fn a_mention_that_names_the_bot_by_id_is_stripped_too() {
    let (zulip, mut received) = FakeZulip::start().await;

    zulip.deliver(json!([channel_message(
        1,
        ALICE,
        "@**Wokesy|99** who is on call",
        &["mentioned"]
    )]));

    assert_eq!(
        within_a_while(received.recv()).await.text.as_str(),
        "who is on call"
    );
}

#[tokio::test]
async fn a_channel_message_that_mentions_the_bot_after_its_start_is_ignored() {
    let (zulip, mut received) = FakeZulip::start().await;

    zulip.deliver(json!([
        channel_message(
            1,
            ALICE,
            "Page @**Wokesy** if it is urgent, not me",
            &["mentioned"]
        ),
        channel_message(
            2,
            ALICE,
            "@**Bob Example** ask @**Wokesy|99** who is on call",
            &["mentioned"]
        ),
        channel_message(3, ALICE, "> @**Wokesy** page db is down", &["mentioned"]),
        channel_message(4, ALICE, " \n @**Wokesy** help", &["mentioned"]),
    ]));

    assert_eq!(
        within_a_while(received.recv()).await,
        received_from(4, ALICE, outage_topic(), "help")
    );
    assert!(received.try_recv().is_err());
}

#[tokio::test]
async fn a_mention_of_a_group_the_bot_belongs_to_or_of_everyone_is_ignored() {
    let (zulip, mut received) = FakeZulip::start().await;

    zulip.deliver(json!([
        channel_message(
            1,
            ALICE,
            "@*sysadmins* page lichess.org is down",
            &["mentioned"]
        ),
        channel_message(
            2,
            ALICE,
            "@**all** page lichess.org is down",
            &["mentioned", "wildcard_mentioned"]
        ),
        channel_message(3, ALICE, "@**Wokesy** who is on call", &["mentioned"]),
    ]));

    assert_eq!(
        within_a_while(received.recv()).await.text.as_str(),
        "who is on call"
    );
    assert!(received.try_recv().is_err());
}

#[tokio::test]
async fn messages_not_addressed_to_the_bot_alone_are_ignored() {
    let (zulip, mut received) = FakeZulip::start().await;

    zulip.deliver(json!([
        channel_message(1, ALICE, "@**Wokesy** help", &["read"]),
        channel_message(2, ALICE, "@**all** help", &["wildcard_mentioned"]),
        group_message(3, BOT, &[BOT, ALICE], "I don't know that command."),
        channel_message(4, BOT, "@**Wokesy** help", &["mentioned"]),
        group_message(5, ALICE, &[ALICE, BOB, BOT], "help"),
        direct_message(6, ALICE, "stop waking me"),
    ]));

    assert_eq!(
        within_a_while(received.recv()).await.text.as_str(),
        "stop waking me"
    );
    assert!(received.try_recv().is_err());
}

#[tokio::test]
async fn each_poll_acknowledges_the_last_event_it_saw() {
    let (mut zulip, mut received) = FakeZulip::start().await;
    assert_eq!(zulip.next_poll().await, first_poll_of("queue-1"));

    zulip.deliver(json!([{ "id": 3, "type": "heartbeat" }]));
    let after_heartbeat = zulip.next_poll().await;
    zulip.deliver(json!([
        channel_message(4, ALICE, "lunch?", &["read"]),
        direct_message(5, ALICE, "help"),
    ]));
    let after_messages = zulip.next_poll().await;

    assert_eq!(after_heartbeat, ("queue-1".to_owned(), "3".to_owned()));
    assert_eq!(after_messages, ("queue-1".to_owned(), "5".to_owned()));
    assert_eq!(within_a_while(received.recv()).await.text.as_str(), "help");
}

#[tokio::test]
async fn an_event_without_an_id_is_ignored_and_logged_without_what_it_holds() {
    let (logs, _listening) = Logs::listen();
    let (zulip, mut received) = FakeZulip::start().await;
    let mut without_an_id = channel_message(
        1,
        ALICE,
        "@**Wokesy** the launch code is 0000",
        &["mentioned"],
    );
    without_an_id.as_object_mut().unwrap().remove("id");
    without_an_id["message"]["sender_email"] = json!("alice@example.com");

    zulip.deliver(json!([without_an_id, direct_message(2, ALICE, "help")]));

    assert_eq!(within_a_while(received.recv()).await.text.as_str(), "help");
    assert!(received.try_recv().is_err());
    let logged = logs.text();
    assert!(
        logged.contains("zulip event without an id or type ignored"),
        "{logged}"
    );
    for secret in [
        "launch code",
        "alice@example.com",
        "Alice Example",
        "sysadmin",
    ] {
        assert!(!logged.contains(secret), "{secret} is in {logged}");
    }
}

#[tokio::test]
async fn an_expired_queue_is_registered_again() {
    let (mut zulip, mut received) = FakeZulip::start().await;
    assert_eq!(zulip.next_poll().await, first_poll_of("queue-1"));

    zulip.expire("queue-1");
    let after_expiry = zulip.next_poll().await;
    zulip.deliver(json!([direct_message(1, ALICE, "help")]));

    assert_eq!(after_expiry, first_poll_of("queue-2"));
    assert_eq!(within_a_while(received.recv()).await.text.as_str(), "help");
}

#[tokio::test]
async fn a_queue_zulip_rejects_before_it_delivered_anything_is_registered_again_after_a_longer_wait_each_time()
 {
    let first_wait = Duration::from_millis(60);
    let (mut zulip, _received) = FakeZulip::start_with(InboxConfig {
        initial_backoff: first_wait,
        max_backoff: Duration::from_secs(1),
    })
    .await;
    assert_eq!(zulip.next_poll().await, first_poll_of("queue-1"));

    let rejected = std::time::Instant::now();
    zulip.expire("queue-1");
    let after_one = zulip.next_poll().await;
    let waited_once = rejected.elapsed();
    let rejected = std::time::Instant::now();
    zulip.expire("queue-2");
    let after_two = zulip.next_poll().await;
    let waited_twice = rejected.elapsed();

    assert_eq!(after_one, first_poll_of("queue-2"));
    assert_eq!(after_two, first_poll_of("queue-3"));
    assert!(waited_once >= first_wait, "{waited_once:?}");
    assert!(waited_twice >= first_wait * 2, "{waited_twice:?}");
}

#[tokio::test]
async fn a_queue_that_expires_after_it_delivered_is_registered_again_without_a_wait() {
    let (mut zulip, mut received) = FakeZulip::start_with(InboxConfig {
        initial_backoff: Duration::from_hours(1),
        max_backoff: Duration::from_hours(1),
    })
    .await;
    assert_eq!(zulip.next_poll().await, first_poll_of("queue-1"));
    zulip.deliver(json!([{ "id": 0, "type": "heartbeat" }]));
    assert_eq!(
        zulip.next_poll().await,
        ("queue-1".to_owned(), "0".to_owned())
    );

    zulip.expire("queue-1");
    let after_expiry = zulip.next_poll().await;
    zulip.deliver(json!([direct_message(1, ALICE, "help")]));

    assert_eq!(after_expiry, first_poll_of("queue-2"));
    assert_eq!(within_a_while(received.recv()).await.text.as_str(), "help");
}

#[tokio::test]
async fn a_failed_poll_is_retried_on_the_same_queue() {
    let (mut zulip, mut received) = FakeZulip::start().await;
    assert_eq!(zulip.next_poll().await, first_poll_of("queue-1"));

    zulip.reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "result": "error", "msg": "Internal server error" }),
    );
    let after_failure = zulip.next_poll().await;
    zulip.deliver(json!([direct_message(1, ALICE, "help")]));

    assert_eq!(after_failure, first_poll_of("queue-1"));
    assert_eq!(within_a_while(received.recv()).await.text.as_str(), "help");
}

#[tokio::test]
async fn reading_ends_once_nobody_receives_the_messages() {
    let (mut zulip, received) = FakeZulip::start().await;
    zulip.next_poll().await;

    drop(received);

    tokio::time::timeout(Duration::from_secs(5), &mut zulip.reading)
        .await
        .unwrap()
        .unwrap();
}
