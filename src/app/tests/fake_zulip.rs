use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::header::USER_AGENT;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc, watch};
use url::Url;

use crate::chat::{ChannelName, ChatUserId, Conversation, TopicName};

pub const BOT: u64 = 99;
pub const ALICE: u64 = 12;
pub const BOB: u64 = 13;
pub const SAM: u64 = 14;

const PATIENCE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Posted {
    pub to: Conversation,
    pub markdown: String,
}

#[derive(Clone)]
struct Organization {
    user_agents: watch::Sender<Vec<String>>,
    posted: watch::Sender<Vec<Posted>>,
    arriving: Arc<Mutex<mpsc::UnboundedReceiver<Value>>>,
    waiting_for_events: watch::Sender<usize>,
    channels_never_answered: watch::Sender<Vec<String>>,
    never_answered: watch::Sender<Vec<Posted>>,
}

pub struct FakeZulip {
    pub url: Url,
    organization: Organization,
    arrives: mpsc::UnboundedSender<Value>,
    arrived: AtomicI64,
}

pub struct Posts {
    posted: watch::Receiver<Vec<Posted>>,
    wanted: Box<dyn Fn(&Conversation) -> bool + Send + Sync>,
    read: usize,
}

struct WaitingForEvents(watch::Sender<usize>);

impl WaitingForEvents {
    fn begin(waiting: &watch::Sender<usize>) -> Self {
        waiting.send_modify(|waiting| *waiting += 1);
        Self(waiting.clone())
    }
}

impl Drop for WaitingForEvents {
    fn drop(&mut self) {
        self.0.send_modify(|waiting| *waiting -= 1);
    }
}

impl FakeZulip {
    pub async fn start() -> Self {
        let (arrives, arriving) = mpsc::unbounded_channel();
        let organization = Organization {
            user_agents: watch::Sender::new(Vec::new()),
            posted: watch::Sender::new(Vec::new()),
            arriving: Arc::new(Mutex::new(arriving)),
            waiting_for_events: watch::Sender::new(0),
            channels_never_answered: watch::Sender::new(Vec::new()),
            never_answered: watch::Sender::new(Vec::new()),
        };
        let router = Router::new()
            .route("/api/v1/users/me", get(own_user))
            .route("/api/v1/users", get(members))
            .route("/api/v1/register", post(register))
            .route("/api/v1/events", get(events))
            .route("/api/v1/messages", post(send_message))
            .layer(middleware::from_fn_with_state(
                organization.clone(),
                note_the_user_agent,
            ))
            .with_state(organization.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            url: format!("http://{address}").parse().unwrap(),
            organization,
            arrives,
            arrived: AtomicI64::new(0),
        }
    }

    pub fn user_agents(&self) -> Vec<String> {
        self.organization.user_agents.borrow().clone()
    }

    pub fn posted(&self) -> Vec<Posted> {
        self.organization.posted.borrow().clone()
    }

    pub fn posts_in(&self, conversation: Conversation) -> Posts {
        self.posts_where(move |to| *to == conversation)
    }

    pub fn posts_it_never_answered(&self) -> Posts {
        Posts {
            posted: self.organization.never_answered.subscribe(),
            wanted: Box::new(|_| true),
            read: 0,
        }
    }

    fn posts_where(&self, wanted: impl Fn(&Conversation) -> bool + Send + Sync + 'static) -> Posts {
        Posts {
            posted: self.organization.posted.subscribe(),
            wanted: Box::new(wanted),
            read: 0,
        }
    }

    pub fn mention(&self, sender: u64, channel: &str, topic: &str, text: &str) {
        self.arrive(
            sender,
            &["mentioned"],
            &format!("@**Wokesy** {text}"),
            json!({ "type": "stream", "display_recipient": channel, "stream_id": 1, "subject": topic }),
        );
    }

    pub fn direct_message(&self, sender: u64, text: &str) {
        self.arrive(
            sender,
            &["read"],
            text,
            json!({
                "type": "private",
                "display_recipient": [{ "id": sender }, { "id": BOT }],
                "subject": "",
            }),
        );
    }

    fn arrive(&self, sender: u64, flags: &[&str], content: &str, place: Value) {
        let id = self.arrived.fetch_add(1, Ordering::Relaxed);
        let mut message = json!({
            "id": 1000 + id,
            "sender_id": sender,
            "sender_full_name": full_name_of(sender),
            "content": content,
        });
        if let (Value::Object(message), Value::Object(place)) = (&mut message, place) {
            message.extend(place);
        }
        self.arrives
            .send(json!({ "id": id, "type": "message", "flags": flags, "message": message }))
            .unwrap();
    }

    pub fn never_answers_posts_in(&self, channel: &str) {
        self.organization
            .channels_never_answered
            .send_modify(|channels| channels.push(channel.to_owned()));
    }

    pub async fn was_sent_a_post_it_never_answered(&self) -> Posted {
        let mut never_answered = self.organization.never_answered.subscribe();
        let all = tokio::time::timeout(PATIENCE, never_answered.wait_for(|all| !all.is_empty()))
            .await
            .unwrap()
            .unwrap();
        all[0].clone()
    }

    pub async fn waits_for_messages(&self) {
        self.long_polls_waiting(1).await;
    }

    pub async fn is_no_longer_read(&self) {
        self.long_polls_waiting(0).await;
    }

    async fn long_polls_waiting(&self, wanted: usize) {
        let mut waiting = self.organization.waiting_for_events.subscribe();
        tokio::time::timeout(PATIENCE, waiting.wait_for(|waiting| *waiting == wanted))
            .await
            .unwrap()
            .unwrap();
    }
}

impl Posts {
    pub async fn next(&mut self) -> String {
        let (wanted, read) = (&self.wanted, self.read);
        let mine = |all: &[Posted]| -> Option<String> {
            all.iter()
                .filter(|posted| wanted(&posted.to))
                .nth(read)
                .map(|posted| posted.markdown.clone())
        };
        let arrived =
            tokio::time::timeout(PATIENCE, self.posted.wait_for(|all| mine(all).is_some()))
                .await
                .map(|all| mine(&all.unwrap()));
        let Ok(Some(next)) = arrived else {
            panic!(
                "post {} did not arrive; posted so far: {:#?}",
                read + 1,
                *self.posted.borrow()
            );
        };
        self.read += 1;
        next
    }
}

fn success(fields: Value) -> Json<Value> {
    let mut reply = json!({ "result": "success", "msg": "" });
    if let (Value::Object(reply), Value::Object(fields)) = (&mut reply, fields) {
        reply.extend(fields);
    }
    Json(reply)
}

fn full_name_of(user_id: u64) -> &'static str {
    match user_id {
        BOT => "Wokesy",
        ALICE => "Alice Example",
        BOB => "Bob Example",
        _ => "Sam Stranger",
    }
}

async fn note_the_user_agent(
    State(organization): State<Organization>,
    request: Request,
    next: Next,
) -> Response {
    let user_agent = String::from_utf8_lossy(request.headers()[USER_AGENT].as_bytes()).into_owned();
    organization
        .user_agents
        .send_modify(|all| all.push(user_agent));
    next.run(request).await
}

async fn own_user() -> Json<Value> {
    success(json!({
        "user_id": BOT,
        "full_name": full_name_of(BOT),
        "email": "wokesy-bot@zulip.example.com",
        "is_bot": true,
    }))
}

async fn members() -> Json<Value> {
    let member = |id: u64, delivery_email: Option<&str>, is_bot: bool| {
        json!({
            "user_id": id,
            "full_name": full_name_of(id),
            "email": format!("user{id}@zulip.example.com"),
            "delivery_email": delivery_email,
            "is_active": true,
            "is_bot": is_bot,
        })
    };
    success(json!({
        "members": [
            member(BOT, None, true),
            member(ALICE, Some("alice@example.com"), false),
            member(BOB, None, false),
            member(SAM, Some("sam@example.com"), false),
        ],
    }))
}

async fn register() -> Json<Value> {
    success(json!({ "queue_id": "queue-1", "last_event_id": -1 }))
}

async fn events(State(organization): State<Organization>) -> Json<Value> {
    let mut arriving = organization.arriving.lock().await;
    let arrived = if let Ok(arrived) = arriving.try_recv() {
        Some(arrived)
    } else {
        let _waiting = WaitingForEvents::begin(&organization.waiting_for_events);
        arriving.recv().await
    };
    match arrived {
        Some(event) => success(json!({ "events": [event] })),
        None => std::future::pending().await,
    }
}

async fn send_message(
    State(organization): State<Organization>,
    Form(message): Form<BTreeMap<String, String>>,
) -> Json<Value> {
    let to = match message["type"].as_str() {
        "direct" => {
            let recipients: Vec<u64> = serde_json::from_str(&message["to"]).unwrap();
            Conversation::Direct {
                user_id: ChatUserId(recipients[0]),
            }
        }
        _ => Conversation::Topic {
            channel: ChannelName::try_new(message["to"].as_str()).unwrap(),
            topic: TopicName::try_new(message["topic"].as_str()).unwrap(),
        },
    };
    let post = Posted {
        to,
        markdown: message["content"].clone(),
    };
    let never_answered = matches!(
        &post.to,
        Conversation::Topic { channel, .. }
            if organization.channels_never_answered.borrow().iter().any(|name| name == channel.as_str())
    );
    if never_answered {
        organization
            .never_answered
            .send_modify(|all| all.push(post));
        return std::future::pending().await;
    }
    let mut id = 0;
    organization.posted.send_modify(|posted| {
        posted.push(post);
        id = posted.len();
    });
    success(json!({ "id": id }))
}
