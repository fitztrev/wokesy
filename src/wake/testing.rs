use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::{Notify, watch};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{Answer, Commands, Config};
use crate::chat::{
    ChannelName, ChatUserId, CommandText, Conversation, IncomingMessage, MessageId, TopicName,
};
use crate::on_call::{Level, Policy, Roster, Shift};
use crate::pagerduty::{
    self, EscalationLevel, EscalationPolicyId, EscalationPolicyName, ScheduleId, ScheduleName,
    ScheduleRef, User, UserId, UserRef,
};
use crate::people::{Directory, Email, Links, Name};

pub const ALICE: ChatUserId = ChatUserId(12);
pub const BOB: ChatUserId = ChatUserId(13);
pub const STRANGER: ChatUserId = ChatUserId(99);

pub const PRIMARY_OVERRIDES: &str = "/schedules/PI7DH85/overrides";
pub const FISHNET_OVERRIDES: &str = "/schedules/PFISHNT/overrides";
pub const PRIMARY_SHIFTS: &str = "/v3/schedules/PI7DH85/custom_shifts";
pub const ENQUEUE: &str = "/v2/enqueue";

pub const ON_CALL_NOW: &str = "\
:pager: **On call now:** @_**|13**

```spoiler schedule
**Lichess · Level 1**
- @_**|13**

**Lichess · Level 2**
- Carol Example
```";

const A_MOMENT: Duration = Duration::from_millis(50);
const SHIFT_BASED: &str = "This is a shift-based schedule. Use the v3 Schedules API to access it.";

pub struct Setup {
    pub config: Config,
    pub paging: bool,
    pub roster: Option<Roster>,
    pub directory: Option<Directory>,
    pub open_channels: Vec<ChannelName>,
}

pub struct Answering {
    pub pagerduty: MockServer,
    pub commands: Commands,
    pub roster: watch::Sender<Option<Roster>>,
    pub directory: watch::Sender<Option<Directory>>,
    pub refresh: Arc<Notify>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            config: waking_on("PI7DH85"),
            paging: true,
            roster: Some(lichess_alone()),
            directory: Some(alice_and_bob_in_chat()),
            open_channels: vec![channel("sysadmin")],
        }
    }
}

impl Setup {
    pub fn with_roster(roster: Roster) -> Self {
        Self {
            roster: Some(roster),
            ..Self::default()
        }
    }

    pub fn waking_on(schedules: &str, roster: Roster) -> Self {
        Self {
            config: waking_on(schedules),
            ..Self::with_roster(roster)
        }
    }

    pub fn waking_on_both_rotas() -> Self {
        Self::waking_on("PFISHNT, PI7DH85", fishnet_and_lichess())
    }

    pub async fn start(self) -> Answering {
        let pagerduty = MockServer::start().await;
        let http = reqwest::Client::new();
        let rest = pagerduty::Config {
            token: SecretString::from("secret"),
            base_url: pagerduty.uri().parse().unwrap(),
            events: None,
        };
        let events = pagerduty::EventsConfig {
            routing_key: SecretString::from("samplekeyhere"),
            url: pagerduty.uri().parse().unwrap(),
        };
        let (roster, rosters) = watch::channel(self.roster);
        let (directory, directories) = watch::channel(self.directory);
        let refresh = Arc::new(Notify::new());
        let commands = Commands::new(
            pagerduty::Api::new(http.clone(), &rest),
            self.paging.then(|| pagerduty::Events::new(http, &events)),
            rosters,
            directories,
            Arc::clone(&refresh),
            self.config,
            self.open_channels,
        );
        Answering {
            pagerduty,
            commands,
            roster,
            directory,
            refresh,
        }
    }
}

impl Answering {
    pub async fn answer(&mut self, message: &IncomingMessage) -> Answer {
        self.commands.answer(message).await
    }

    pub async fn reply(&mut self, message: &IncomingMessage) -> String {
        self.answer(message).await.reply.markdown().to_owned()
    }

    pub async fn requests(&self) -> Vec<String> {
        self.pagerduty
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| format!("{} {}", request.method, request.url.path()))
            .collect()
    }

    pub async fn posted(&self) -> Vec<Value> {
        self.pagerduty
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.method == "POST")
            .map(|request| request.body_json().unwrap())
            .collect()
    }

    pub async fn was_asked_to_refresh(&self) -> bool {
        tokio::time::timeout(A_MOMENT, self.refresh.notified())
            .await
            .is_ok()
    }

    pub async fn accepts_pages(&self) {
        Mock::given(method("POST"))
            .and(path(ENQUEUE))
            .respond_with(ResponseTemplate::new(202).set_body_json(json!({
                "status": "success",
                "message": "Event processed",
                "dedup_key": "srv01/HTTP",
            })))
            .mount(&self.pagerduty)
            .await;
    }

    pub async fn creates_overrides(&self, overrides: &str, ids: &[&str]) {
        for id in ids {
            Mock::given(method("POST"))
                .and(path(overrides))
                .respond_with(
                    ResponseTemplate::new(201).set_body_json(json!({ "override": { "id": id } })),
                )
                .up_to_n_times(1)
                .mount(&self.pagerduty)
                .await;
        }
    }

    pub async fn refuses_overrides(&self, overrides: &str) {
        Mock::given(method("POST"))
            .and(path(overrides))
            .respond_with(forbidden())
            .mount(&self.pagerduty)
            .await;
    }

    pub async fn is_shift_based(&self, overrides: &str) {
        Mock::given(method("POST"))
            .and(path(overrides))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": { "message": SHIFT_BASED, "code": 3005, "errors": [SHIFT_BASED] },
            })))
            .mount(&self.pagerduty)
            .await;
    }

    pub async fn creates_custom_shifts(&self, custom_shifts: &str, ids: &[&str]) {
        for id in ids {
            Mock::given(method("POST"))
                .and(path(custom_shifts))
                .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                    "custom_shifts": [{ "id": id, "type": "custom_shift" }],
                })))
                .up_to_n_times(1)
                .mount(&self.pagerduty)
                .await;
        }
    }

    pub async fn deletes_overrides(&self, overrides: &str, ids: &[&str]) {
        for id in ids {
            self.answers_a_delete(overrides, id, ResponseTemplate::new(204))
                .await;
        }
    }

    pub async fn answers_a_delete(&self, made: &str, id: &str, answer: ResponseTemplate) {
        Mock::given(method("DELETE"))
            .and(path(format!("{made}/{id}")))
            .respond_with(answer)
            .up_to_n_times(1)
            .mount(&self.pagerduty)
            .await;
    }
}

pub fn waking_on(schedules: &str) -> Config {
    Config {
        schedules: schedules.into(),
        ..Config::default()
    }
}

pub fn forbidden() -> ResponseTemplate {
    ResponseTemplate::new(403)
        .set_body_json(json!({ "error": { "message": "Forbidden", "code": 2010 } }))
}

pub fn channel(name: &str) -> ChannelName {
    ChannelName::try_new(name).unwrap()
}

pub fn topic(channel_name: &str, topic: &str) -> Conversation {
    Conversation::Topic {
        channel: channel(channel_name),
        topic: TopicName::try_new(topic).unwrap(),
    }
}

pub fn message(sender: ChatUserId, reply_to: Conversation, text: &str) -> IncomingMessage {
    let sender_name = match sender {
        ALICE => "Alice Example",
        BOB => "Bob Example",
        _ => "Sam Stranger",
    };
    IncomingMessage {
        id: MessageId(1),
        sender,
        sender_name: Name::try_new(sender_name).unwrap(),
        reply_to,
        text: CommandText::new(text),
    }
}

pub fn mention_in(channel: &str, sender: ChatUserId, text: &str) -> IncomingMessage {
    message(sender, topic(channel, "incident"), text)
}

pub fn in_sysadmin(sender: ChatUserId, text: &str) -> IncomingMessage {
    mention_in("sysadmin", sender, text)
}

pub fn direct(sender: ChatUserId, text: &str) -> IncomingMessage {
    message(sender, Conversation::Direct { user_id: sender }, text)
}

pub fn user(id: &str, name: &str) -> UserRef {
    UserRef {
        id: UserId::try_new(id).unwrap(),
        name: Name::try_new(name).unwrap(),
    }
}

pub fn schedule(id: &str, name: &str) -> ScheduleRef {
    ScheduleRef {
        id: ScheduleId::try_new(id).unwrap(),
        name: ScheduleName::try_new(name).unwrap(),
    }
}

pub fn primary_rota() -> ScheduleRef {
    schedule("PI7DH85", "Primary rota")
}

pub fn fishnet_rota() -> ScheduleRef {
    schedule("PFISHNT", "Fishnet rota")
}

pub fn level(number: u32, schedules: &[ScheduleRef], people: &[UserRef]) -> Level {
    Level {
        number: EscalationLevel::new(number),
        schedules: schedules.to_vec(),
        people: people.to_vec(),
        shifts: vec![],
    }
}

pub fn with_on_call(level: Level, users: &[UserRef]) -> Level {
    let shifts = users
        .iter()
        .map(|user| Shift {
            user: user.clone(),
            ends: None,
        })
        .collect();
    Level { shifts, ..level }
}

pub fn on_call_at(number: u32, users: &[UserRef]) -> Level {
    with_on_call(level(number, &[], &[]), users)
}

pub fn crowd(of: usize) -> Vec<UserRef> {
    (1..=of)
        .map(|number| user(&format!("PCROWD{number}"), &format!("Person {number}")))
        .collect()
}

pub fn only_policy(levels: &[Level]) -> Roster {
    Roster {
        policies: vec![policy("PDEFAUL", "Default", levels)],
    }
}

pub fn policy(id: &str, name: &str, levels: &[Level]) -> Policy {
    Policy {
        id: EscalationPolicyId::try_new(id).unwrap(),
        name: EscalationPolicyName::try_new(name).unwrap(),
        levels: levels.to_vec(),
    }
}

pub fn lichess_alone() -> Roster {
    let bob = user("PBOB001", "Bob Example");
    let carol = user("PCAROL1", "Carol Example");
    let first = Level {
        shifts: vec![Shift {
            user: bob,
            ends: None,
        }],
        ..level(1, &[primary_rota()], &[])
    };
    let second = Level {
        shifts: vec![Shift {
            user: carol.clone(),
            ends: None,
        }],
        ..level(2, &[], &[carol])
    };
    Roster {
        policies: vec![policy("PLICHES", "Lichess", &[first, second])],
    }
}

pub fn fishnet_and_lichess() -> Roster {
    Roster {
        policies: vec![
            policy("PFISHNE", "Fishnet", &[level(1, &[fishnet_rota()], &[])]),
            policy("PLICHES", "Lichess", &[level(1, &[primary_rota()], &[])]),
        ],
    }
}

pub fn alice_and_bob_in_chat() -> Directory {
    directory(&[
        ("PALICE1", "Alice Example", Some(ALICE)),
        ("PBOB001", "Bob Example", Some(BOB)),
        ("PCAROL1", "Carol Example", None),
    ])
}

pub fn directory(people: &[(&str, &str, Option<ChatUserId>)]) -> Directory {
    let email = |id: &str| Email::try_new(format!("{id}@example.com")).unwrap();
    let users: Vec<User> = people
        .iter()
        .map(|(id, name, _)| User {
            id: UserId::try_new(*id).unwrap(),
            name: Name::try_new(*name).unwrap(),
            email: email(id),
        })
        .collect();
    let links: Links = people
        .iter()
        .filter_map(|(id, _, chat_user)| Some(format!("{}={}", email(id), (*chat_user)?.0)))
        .collect::<Vec<_>>()
        .join(",")
        .parse()
        .unwrap();
    Directory::matching(&users, &[], &links)
}
