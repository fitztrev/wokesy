use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;

use super::*;
use crate::chat::{ChannelName, DeliveryError};

const PATIENCE: Duration = Duration::from_secs(5);
const WORKS: &str = ":check: PagerDuty works";
const CANNOT_USE: &str = ":warning: Cannot use PagerDuty, will keep trying: pagerduty rejected the request (503 Service Unavailable): Service Unavailable";

#[derive(Clone, Debug, Default)]
struct RecordingMessenger {
    deliveries: Arc<Mutex<Vec<(Conversation, OutgoingMessage)>>>,
    unreachable: Arc<AtomicBool>,
}

#[derive(Debug, thiserror::Error)]
#[error("unreachable")]
struct Unreachable;

impl RecordingMessenger {
    fn deliveries(&self) -> Vec<(Conversation, OutgoingMessage)> {
        self.deliveries.lock().unwrap().clone()
    }

    fn posted(&self) -> Vec<String> {
        self.deliveries()
            .into_iter()
            .map(|(_, message)| message.markdown().to_owned())
            .collect()
    }

    fn set_unreachable(&self, unreachable: bool) {
        self.unreachable.store(unreachable, Ordering::SeqCst);
    }
}

impl Messenger for RecordingMessenger {
    fn send(
        &self,
        conversation: &Conversation,
        message: &OutgoingMessage,
    ) -> impl Future<Output = Result<(), DeliveryError>> + Send {
        let outcome = if self.unreachable.load(Ordering::SeqCst) {
            Err(DeliveryError {
                conversation: conversation.clone(),
                cause: Box::new(Unreachable),
            })
        } else {
            self.deliveries
                .lock()
                .unwrap()
                .push((conversation.clone(), message.clone()));
            Ok(())
        };
        std::future::ready(outcome)
    }
}

fn everything_available() -> Health {
    Health {
        pagerduty: Availability::Available,
        zulip: Availability::Available,
    }
}

fn unavailable(recovery: Recovery, error: &str) -> Availability {
    Availability::Unavailable(Unavailable {
        recovery,
        error: error.to_owned(),
    })
}

fn unavailable_for_now() -> Availability {
    unavailable(
        Recovery::Recoverable,
        "pagerduty rejected the request (503 Service Unavailable): Service Unavailable",
    )
}

fn pagerduty(pagerduty: Availability) -> Health {
    Health {
        pagerduty,
        ..everything_available()
    }
}

fn channel() -> ChannelName {
    ChannelName::try_new("wokesy-status").unwrap()
}

fn configured(messenger: &RecordingMessenger) -> Status<RecordingMessenger> {
    Status::new(messenger.clone(), Some(Config { channel: channel() })).unwrap()
}

fn topic(status: &Status<RecordingMessenger>) -> String {
    match &status.conversation {
        Some(Conversation::Topic { topic, .. }) => topic.as_str().to_owned(),
        other => panic!("status posts to {other:?}"),
    }
}

struct Following {
    health: watch::Sender<Option<Health>>,
    status: JoinHandle<Status<RecordingMessenger>>,
}

impl Following {
    async fn until_health_is_no_longer_checked(self) -> Status<RecordingMessenger> {
        drop(self.health);
        tokio::time::timeout(PATIENCE, self.status)
            .await
            .unwrap()
            .unwrap()
    }
}

fn following(mut status: Status<RecordingMessenger>, health: Health) -> Following {
    let (health, changes) = watch::channel(Some(health));
    Following {
        health,
        status: tokio::spawn(async move {
            status.follow(changes).await;
            status
        }),
    }
}

async fn until_posted(messenger: &RecordingMessenger, markdown: &str) {
    tokio::time::timeout(PATIENCE, async {
        while !messenger.posted().iter().any(|posted| posted == markdown) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

fn today() -> String {
    let now = humantime::format_rfc3339_seconds(SystemTime::now()).to_string();
    now.split_once('T').unwrap().0.to_owned()
}

#[tokio::test]
async fn startup_posts_the_version_and_changelog_to_the_instance_topic_in_the_configured_channel() {
    let messenger = RecordingMessenger::default();
    let status = configured(&messenger);
    let topic = TopicName::try_new(topic(&status)).unwrap();

    status.started().await;

    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        messenger.deliveries(),
        [(
            Conversation::Topic {
                channel: channel(),
                topic,
            },
            OutgoingMessage::new(format!(
                ":rocket: wokesy v{version} started — [changelog](https://github.com/lakinwecker/wokesy/releases/tag/v{version})"
            )),
        )]
    );
}

#[tokio::test]
async fn the_topic_names_the_version_a_fresh_instance_id_and_the_startup_date() {
    let topic = topic(&configured(&RecordingMessenger::default()));

    let parts: Vec<&str> = topic.split(' ').collect();
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    assert_eq!(parts.len(), 3, "{topic}");
    assert_eq!(parts[0], version);
    assert_eq!(Uuid::parse_str(parts[1]).unwrap().get_version_num(), 4);
    assert_eq!(parts[2], today());
}

#[tokio::test]
async fn the_topic_fits_within_the_zulip_topic_length_limit() {
    let topic = topic(&configured(&RecordingMessenger::default()));

    assert!(topic.chars().count() <= 60, "{topic} is too long");
}

#[tokio::test]
async fn every_instance_gets_its_own_topic() {
    let messenger = RecordingMessenger::default();

    assert_ne!(
        topic(&configured(&messenger)),
        topic(&configured(&messenger))
    );
}

#[tokio::test]
async fn health_changes_are_posted_in_the_same_topic_as_the_startup() {
    let messenger = RecordingMessenger::default();
    let status = configured(&messenger);
    status.started().await;

    following(status, everything_available())
        .until_health_is_no_longer_checked()
        .await;

    let deliveries = messenger.deliveries();
    assert_eq!(deliveries.len(), 2);
    assert_eq!(deliveries[1].0, deliveries[0].0);
    assert_eq!(deliveries[1].1.markdown(), WORKS);
}

#[tokio::test]
async fn when_zulip_and_pagerduty_stop_working_then_only_pagerduty_is_posted() {
    let messenger = RecordingMessenger::default();
    let following = following(configured(&messenger), everything_available());
    until_posted(&messenger, WORKS).await;

    following.health.send_replace(Some(Health {
        pagerduty: unavailable_for_now(),
        zulip: unavailable_for_now(),
    }));
    following.until_health_is_no_longer_checked().await;

    assert_eq!(messenger.posted(), [WORKS, CANNOT_USE]);
}

#[tokio::test]
async fn when_only_zulip_stops_working_then_nothing_is_posted() {
    let messenger = RecordingMessenger::default();
    let following = following(configured(&messenger), everything_available());
    until_posted(&messenger, WORKS).await;

    following.health.send_replace(Some(Health {
        zulip: unavailable_for_now(),
        ..everything_available()
    }));
    following.until_health_is_no_longer_checked().await;

    assert_eq!(messenger.posted(), [WORKS]);
}

#[tokio::test]
async fn when_pagerduty_works_again_then_it_is_posted_and_a_first_health_posts_that_it_works() {
    let messenger = RecordingMessenger::default();
    let following = following(configured(&messenger), everything_available());

    until_posted(&messenger, WORKS).await;
    following
        .health
        .send_replace(Some(pagerduty(unavailable_for_now())));
    until_posted(&messenger, CANNOT_USE).await;
    following.health.send_replace(Some(everything_available()));
    following.until_health_is_no_longer_checked().await;

    assert_eq!(
        messenger.posted(),
        [WORKS, CANNOT_USE, ":check: PagerDuty works again"]
    );
}

#[tokio::test]
async fn when_pagerduty_refuses_wokesy_then_a_person_is_asked_to_fix_it() {
    let messenger = RecordingMessenger::default();
    let following = following(configured(&messenger), pagerduty(unavailable_for_now()));
    until_posted(&messenger, CANNOT_USE).await;

    following.health.send_replace(Some(pagerduty(unavailable(
        Recovery::NeedsAPerson,
        "pagerduty rejected the request (401 Unauthorized): Unauthorized",
    ))));
    following.until_health_is_no_longer_checked().await;

    assert_eq!(
        messenger.posted(),
        [
            CANNOT_USE,
            ":rotating_light: PagerDuty refuses wokesy, a person must fix it: pagerduty rejected the request (401 Unauthorized): Unauthorized",
        ]
    );
}

#[tokio::test]
async fn while_pagerduty_stays_unavailable_for_another_reason_the_warning_is_not_repeated() {
    let messenger = RecordingMessenger::default();
    let following = following(configured(&messenger), pagerduty(unavailable_for_now()));
    until_posted(&messenger, CANNOT_USE).await;

    following.health.send_replace(Some(pagerduty(unavailable(
        Recovery::Recoverable,
        "no answer within 5s",
    ))));
    following.until_health_is_no_longer_checked().await;

    assert_eq!(messenger.posted(), [CANNOT_USE]);
}

#[tokio::test(start_paused = true)]
async fn a_requested_shutdown_names_the_signal_and_the_whole_seconds_of_uptime() {
    let messenger = RecordingMessenger::default();
    let status = configured(&messenger);
    tokio::time::advance(Duration::from_millis(3_723_400)).await;

    status
        .stopping(Shutdown::Requested(Signal::Terminate))
        .await;

    assert_eq!(
        messenger.posted(),
        [":octagonal_sign: Shutting down (SIGTERM) after 1h 2m 3s"]
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_shutdown_names_the_error() {
    let messenger = RecordingMessenger::default();
    let status = configured(&messenger);
    tokio::time::advance(Duration::from_secs(90)).await;

    status
        .stopping(Shutdown::Failed(
            "on-call sync stopped unexpectedly".to_owned(),
        ))
        .await;

    assert_eq!(
        messenger.posted(),
        [":octagonal_sign: Shutting down after 1m 30s: on-call sync stopped unexpectedly"]
    );
}

#[tokio::test]
async fn without_a_status_channel_nothing_is_sent() {
    let messenger = RecordingMessenger::default();
    let status = Status::new(messenger.clone(), None).unwrap();

    status.started().await;
    let status = following(status, everything_available())
        .until_health_is_no_longer_checked()
        .await;
    status
        .stopping(Shutdown::Requested(Signal::Interrupt))
        .await;

    assert_eq!(messenger.deliveries(), []);
}

#[tokio::test]
async fn a_failed_delivery_does_not_stop_the_status() {
    let messenger = RecordingMessenger::default();
    messenger.set_unreachable(true);
    let status = configured(&messenger);
    status.started().await;
    let status = following(status, pagerduty(unavailable_for_now()))
        .until_health_is_no_longer_checked()
        .await;
    messenger.set_unreachable(false);

    following(status, everything_available())
        .until_health_is_no_longer_checked()
        .await;

    assert_eq!(messenger.posted(), [":check: PagerDuty works again"]);
}
