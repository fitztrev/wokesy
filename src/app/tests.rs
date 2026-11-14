mod fake_pagerduty;
mod fake_zulip;

use std::net::SocketAddr;

use axum::http::{Method, StatusCode};
use secrecy::SecretString;
use serde_json::json;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::oneshot;

use super::*;
use crate::chat::{ChannelName, ChatUserId, Conversation, TopicName};
use crate::status;
use fake_pagerduty::{ENQUEUE, FakePagerDuty, ON_CALL, PRIMARY_ROTA_OVERRIDES};
use fake_zulip::{ALICE, BOB, FakeZulip, SAM};

const PATIENCE: Duration = Duration::from_secs(5);
const LONGER_THAN_A_TEST: Duration = Duration::from_hours(1);
const ROUTING_KEY: &str = "samplekeyhere";
const SYSADMIN: &str = "sysadmin";
const STATUS: &str = "wokesy status";
const ON_CALL_NOW: &str = "\
:pager: **On call now**
- **Lichess**, level 1: @_**|13** until <time:2026-11-14T20:00:00Z>
- **Lichess**, level 2: Carol Example";

struct Wokesy {
    pagerduty: FakePagerDuty,
    zulip: FakeZulip,
    probes: SocketAddr,
    shutdown: oneshot::Sender<Signal>,
    running: JoinHandle<anyhow::Result<()>>,
}

fn channel(name: &str) -> ChannelName {
    ChannelName::try_new(name).unwrap()
}

fn topic(name: &str) -> Conversation {
    Conversation::Topic {
        channel: channel(SYSADMIN),
        topic: TopicName::try_new(name).unwrap(),
    }
}

fn on_call_topic() -> Conversation {
    topic("on call")
}

fn incident_topic() -> Conversation {
    topic("incident")
}

fn settings(pagerduty: &FakePagerDuty, zulip: &FakeZulip) -> Config {
    Config {
        http: http::Config::default(),
        zulip: zulip::Config {
            site: zulip.url.clone(),
            email: zulip::BotEmail::try_new("wokesy-bot@zulip.example.com").unwrap(),
            api_key: SecretString::from("key"),
            inbox: zulip::InboxConfig {
                initial_backoff: Duration::from_millis(10),
                max_backoff: Duration::from_millis(50),
            },
        },
        pagerduty: pagerduty::Config {
            token: SecretString::from("secret"),
            base_url: pagerduty.url.clone(),
            events: Some(pagerduty::EventsConfig {
                routing_key: SecretString::from(ROUTING_KEY),
                url: pagerduty.url.clone(),
            }),
        },
        announcements: on_call::Announcements {
            channel: channel(SYSADMIN),
            topic: TopicName::try_new("on call").unwrap(),
        },
        on_call: on_call::Config {
            refresh_interval: LONGER_THAN_A_TEST,
        },
        people: people::Config {
            refresh_interval: LONGER_THAN_A_TEST,
            ..people::Config::default()
        },
        wake: wake::Config::default(),
        status: Some(status::Config {
            channel: channel(STATUS),
        }),
        health: health::Config {
            interval: LONGER_THAN_A_TEST,
            ..health::Config::default()
        },
        telemetry: telemetry::Config::default(),
    }
}

impl Wokesy {
    async fn start() -> Self {
        let pagerduty = FakePagerDuty::start().await;
        let zulip = FakeZulip::start().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let probes = listener.local_addr().unwrap();
        let (shutdown, requested) = oneshot::channel();
        let running = tokio::spawn(run_until(
            settings(&pagerduty, &zulip),
            listener,
            async move { requested.await.unwrap_or(Signal::Interrupt) },
        ));
        Self {
            pagerduty,
            zulip,
            probes,
            shutdown,
            running,
        }
    }

    async fn settled() -> Self {
        let wokesy = Self::start().await;
        let mut status = wokesy.zulip.posts_in_channel(STATUS);
        wokesy.zulip.posts_in(on_call_topic()).next().await;
        status.next().await;
        status.next().await;
        wokesy
    }

    async fn probe(&self, path: &str) -> StatusCode {
        reqwest::get(format!("http://{}{path}", self.probes))
            .await
            .unwrap()
            .status()
    }

    async fn shut_down(self) -> (FakePagerDuty, FakeZulip, anyhow::Result<()>) {
        self.shutdown.send(Signal::Terminate).unwrap();
        let outcome = tokio::time::timeout(PATIENCE, self.running)
            .await
            .unwrap()
            .unwrap();
        (self.pagerduty, self.zulip, outcome)
    }
}

#[tokio::test]
async fn who_is_on_call_is_announced_after_start_naming_matched_people_silently() {
    let wokesy = Wokesy::start().await;

    let announced = wokesy.zulip.posts_in(on_call_topic()).next().await;

    assert_eq!(announced, ON_CALL_NOW);
}

#[tokio::test]
async fn a_mention_asking_who_is_on_call_is_answered_in_its_topic() {
    let wokesy = Wokesy::settled().await;

    wokesy
        .zulip
        .mention(ALICE, SYSADMIN, "incident", "who is on call");

    let answer = wokesy.zulip.posts_in(incident_topic()).next().await;
    assert_eq!(answer, ON_CALL_NOW);
}

#[tokio::test]
async fn wake_me_overrides_the_first_line_schedule_and_the_change_is_announced_straight_away() {
    let wokesy = Wokesy::settled().await;
    let mut announcements = wokesy.zulip.posts_in(on_call_topic());
    assert_eq!(announcements.next().await, ON_CALL_NOW);

    wokesy
        .zulip
        .mention(ALICE, SYSADMIN, "incident", "wake me for 4 hours");

    let answer = wokesy.zulip.posts_in(incident_topic()).next().await;
    let announced = announcements.next().await;
    let asked = wokesy.pagerduty.asked();
    let created = wokesy.pagerduty.changes_asked();
    let [created] = created.as_slice() else {
        panic!("expected one change in PagerDuty, got {created:#?}");
    };
    let shift = &created.body["override"];
    let moment = |field: &str| OffsetDateTime::parse(shift[field].as_str().unwrap(), &Rfc3339);
    let until = shift["end"].as_str().unwrap();
    assert!(created.is(&Method::POST, PRIMARY_ROTA_OVERRIDES));
    assert_eq!(
        shift["user"],
        json!({ "id": fake_pagerduty::ALICE, "type": "user_reference" })
    );
    assert_eq!(
        moment("end").unwrap() - moment("start").unwrap(),
        Duration::from_hours(4)
    );
    assert_eq!(
        answer,
        format!(
            ":alarm_clock: I'll wake you for anything until <time:{until}>.\n\
             - **Lichess**: you are first in line through **Primary rota**.\n\
             PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner."
        )
    );
    assert_eq!(
        announced,
        format!(
            ":pager: **On call changed**\n\
             - **Lichess**, level 1: @_**|{ALICE}** until <time:{until}> (was @_**|{BOB}**)"
        )
    );
    let overridden = asked.iter().position(|asked| asked == created).unwrap();
    assert!(
        asked[overridden..]
            .iter()
            .any(|asked| asked.is(&Method::GET, ON_CALL))
    );
}

#[tokio::test]
async fn a_direct_message_is_pointed_to_the_open_channels_and_changes_nothing_in_pagerduty() {
    let wokesy = Wokesy::settled().await;

    wokesy.zulip.direct_message(ALICE, "wake me for 4 hours");

    let directly = Conversation::Direct {
        user_id: ChatUserId(ALICE),
    };
    let answer = wokesy.zulip.posts_in(directly).next().await;
    assert_eq!(
        answer,
        "I only take commands in the open, in #**sysadmin**. Mention me there."
    );
    assert_eq!(wokesy.pagerduty.changes_asked(), []);
}

#[tokio::test]
async fn a_page_alerts_through_the_events_api_and_is_quoted_in_the_on_call_topic() {
    let wokesy = Wokesy::settled().await;
    let mut announcements = wokesy.zulip.posts_in(on_call_topic());
    assert_eq!(announcements.next().await, ON_CALL_NOW);

    wokesy
        .zulip
        .mention(SAM, SYSADMIN, "incident", "page db is down");

    let answer = wokesy.zulip.posts_in(incident_topic()).next().await;
    let announced = announcements.next().await;
    let changes = wokesy.pagerduty.changes_asked();
    let [alert] = changes.as_slice() else {
        panic!("expected one alert in PagerDuty, got {changes:#?}");
    };
    assert!(alert.is(&Method::POST, ENQUEUE));
    assert_eq!(
        alert.body,
        json!({
            "routing_key": ROUTING_KEY,
            "event_action": "trigger",
            "payload": {
                "summary": "Sam Stranger: db is down",
                "source": "Zulip",
                "severity": "critical",
                "custom_details": { "paged_by": "Sam Stranger" },
            },
        })
    );
    assert_eq!(
        answer,
        format!(
            ":rotating_light: Paged. PagerDuty is alerting whoever is on call.\n\n{ON_CALL_NOW}"
        )
    );
    assert_eq!(
        announced,
        format!(":rotating_light: @_**|{SAM}** paged on call:\n> db is down")
    );
}

#[tokio::test]
async fn every_request_to_pagerduty_and_zulip_says_it_comes_from_wokesy() {
    let wokesy = Wokesy::settled().await;
    wokesy
        .zulip
        .mention(SAM, SYSADMIN, "incident", "page db is down");
    wokesy.zulip.posts_in(incident_topic()).next().await;

    let mut user_agents = wokesy.zulip.user_agents();
    user_agents.extend(
        wokesy
            .pagerduty
            .asked()
            .into_iter()
            .map(|asked| asked.user_agent),
    );
    user_agents.dedup();

    assert_eq!(user_agents, [USER_AGENT]);
    assert_eq!(
        USER_AGENT,
        concat!(
            "wokesy/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/lichess-org/wokesy)"
        )
    );
}

#[tokio::test]
async fn readyz_answers_200_once_pagerduty_was_found_available() {
    let wokesy = Wokesy::start().await;
    let mut status = wokesy.zulip.posts_in_channel(STATUS);

    assert!(status.next().await.starts_with(concat!(
        ":rocket: wokesy v",
        env!("CARGO_PKG_VERSION"),
        " started"
    )));
    assert_eq!(status.next().await, ":check: PagerDuty works");
    assert_eq!(wokesy.probe("/readyz").await, StatusCode::OK);
    assert_eq!(wokesy.probe("/livez").await, StatusCode::OK);
}

#[tokio::test]
async fn a_requested_shutdown_is_posted_and_ends_without_an_error() {
    let wokesy = Wokesy::settled().await;

    let (_pagerduty, zulip, outcome) = wokesy.shut_down().await;

    let last = zulip.posted().pop().unwrap();
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(
        matches!(&last.to, Conversation::Topic { channel, .. } if channel.as_str() == STATUS),
        "{last:?}"
    );
    assert!(
        last.markdown
            .starts_with(":octagonal_sign: Shutting down (SIGTERM) after "),
        "{last:?}"
    );
}

#[tokio::test]
async fn shutdown_stops_reading_first_and_finishes_the_command_in_hand_before_it_says_goodbye() {
    let wokesy = Wokesy::settled().await;
    wokesy.pagerduty.holds_pages_back();
    wokesy
        .zulip
        .mention(SAM, SYSADMIN, "incident", "page db is down");
    wokesy.pagerduty.was_asked(Method::POST, ENQUEUE).await;
    wokesy.zulip.waits_for_messages().await;

    let Wokesy {
        pagerduty,
        zulip,
        shutdown,
        running,
        ..
    } = wokesy;
    shutdown.send(Signal::Terminate).unwrap();
    zulip.is_no_longer_read().await;
    pagerduty.lets_pages_through();
    let outcome = tokio::time::timeout(PATIENCE, running)
        .await
        .unwrap()
        .unwrap();

    let posted: Vec<String> = zulip
        .posted()
        .into_iter()
        .map(|posted| posted.markdown)
        .collect();
    let [.., answer, announced, goodbye] = posted.as_slice() else {
        panic!("expected an answer, an announcement, and a goodbye, got {posted:#?}");
    };
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(answer.starts_with(":rotating_light: Paged."), "{answer}");
    assert!(
        announced.starts_with(&format!(":rotating_light: @_**|{SAM}** paged on call:")),
        "{announced}"
    );
    assert!(
        goodbye.starts_with(":octagonal_sign: Shutting down (SIGTERM) after "),
        "{goodbye}"
    );
}

#[tokio::test]
async fn a_job_that_ends_by_itself_is_an_error_that_names_it() {
    let exit = tokio::spawn(async {}).await;

    let error = ended(ON_CALL_SYNC, exit);

    assert_eq!(format!("{error:#}"), "on-call sync stopped unexpectedly");
}

#[tokio::test]
async fn a_job_that_panics_is_an_error_that_names_it_and_carries_the_panic() {
    let exit = tokio::spawn(async { panic!("the roster is on fire") }).await;

    let error = format!("{:#}", ended(ON_CALL_SYNC, exit));

    assert!(error.starts_with("on-call sync failed: "), "{error}");
    assert!(error.contains("the roster is on fire"), "{error}");
}
