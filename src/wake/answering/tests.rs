use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinHandle;

use super::*;
use crate::chat::{AcknowledgementError, DeliveryError, MessageId};
use crate::testing::Logs;
use crate::wake::testing::{
    ALICE, PRIMARY_OVERRIDES, Setup, in_sysadmin, level, message, only_policy, primary_rota, topic,
    user,
};

const PATIENCE: Duration = Duration::from_secs(5);
const HELP_STARTS: &str = "I manage who is on call in PagerDuty.";
const PAGED_STARTS: &str = ":rotating_light: Paged @_**|13**.";
const ANNOUNCEMENT: &str = ":rotating_light: @_**|12** paged on call:\n> lichess.org is down";
const NOW_ON_CALL: &str =
    ":alarm_clock: @_**|12** is now on call for 4 hours. Level 2 (1 person) is backup.";
const NOW_ON_CALL_WITH_DAVE: &str =
    ":alarm_clock: @_**|12** is now on call for 4 hours, together with 1 other at level 1.";
const WAKE_ME: MessageId = MessageId(77);

#[derive(Clone, Debug)]
struct RecordingMessenger {
    attempts: mpsc::UnboundedSender<Attempt>,
    unreachable: Arc<AtomicBool>,
    refuses_check_marks: Arc<AtomicBool>,
}

#[derive(Debug, PartialEq, Eq)]
enum Attempt {
    Message {
        to: Conversation,
        markdown: String,
        delivered: bool,
    },
    CheckMark {
        on: MessageId,
        added: bool,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("unreachable")]
struct Unreachable;

impl Messenger for RecordingMessenger {
    fn send(
        &self,
        conversation: &Conversation,
        message: &OutgoingMessage,
    ) -> impl Future<Output = Result<(), DeliveryError>> + Send {
        let delivered = !self.unreachable.load(Ordering::SeqCst);
        self.attempts
            .send(Attempt::Message {
                to: conversation.clone(),
                markdown: message.markdown().to_owned(),
                delivered,
            })
            .unwrap();
        std::future::ready(if delivered {
            Ok(())
        } else {
            Err(DeliveryError {
                conversation: conversation.clone(),
                cause: Box::new(Unreachable),
            })
        })
    }
}

impl Acknowledger for RecordingMessenger {
    fn acknowledge(
        &self,
        message: MessageId,
    ) -> impl Future<Output = Result<(), AcknowledgementError>> + Send {
        let added = !self.refuses_check_marks.load(Ordering::SeqCst);
        self.attempts
            .send(Attempt::CheckMark { on: message, added })
            .unwrap();
        std::future::ready(if added {
            Ok(())
        } else {
            Err(AcknowledgementError {
                message,
                cause: Box::new(Unreachable),
            })
        })
    }
}

struct Chat {
    task: JoinHandle<()>,
    incoming: mpsc::Sender<IncomingMessage>,
    attempts: mpsc::UnboundedReceiver<Attempt>,
    unreachable: Arc<AtomicBool>,
    refuses_check_marks: Arc<AtomicBool>,
}

fn on_call_topic() -> Conversation {
    topic("sysadmin", "on call")
}

impl Chat {
    async fn start() -> Self {
        Self::start_with(Setup::default()).await
    }

    async fn start_with(setup: Setup) -> Self {
        let answering = setup.start().await;
        answering.accepts_pages().await;
        answering
            .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
            .await;
        let (attempted, attempts) = mpsc::unbounded_channel();
        let unreachable = Arc::new(AtomicBool::new(false));
        let refuses_check_marks = Arc::new(AtomicBool::new(false));
        let messenger = RecordingMessenger {
            attempts: attempted,
            unreachable: Arc::clone(&unreachable),
            refuses_check_marks: Arc::clone(&refuses_check_marks),
        };
        let (incoming, messages) = mpsc::channel(8);
        let task = tokio::spawn(async move {
            let _pagerduty = answering.pagerduty;
            let _published = (answering.roster, answering.directory);
            keep_answering(answering.commands, messenger, on_call_topic(), messages).await;
        });
        Self {
            task,
            incoming,
            attempts,
            unreachable,
            refuses_check_marks,
        }
    }

    async fn says(&self, message: IncomingMessage) {
        self.incoming.send(message).await.unwrap();
    }

    async fn next_attempt(&mut self) -> Attempt {
        tokio::time::timeout(PATIENCE, self.attempts.recv())
            .await
            .unwrap()
            .unwrap()
    }

    async fn next_delivery(&mut self) -> (Conversation, String) {
        match self.next_attempt().await {
            Attempt::Message {
                to,
                markdown,
                delivered: true,
            } => (to, markdown),
            attempt => panic!("expected a delivered message, got {attempt:?}"),
        }
    }

    async fn stops(self) -> mpsc::UnboundedReceiver<Attempt> {
        drop(self.incoming);
        tokio::time::timeout(PATIENCE, self.task)
            .await
            .unwrap()
            .unwrap();
        self.attempts
    }
}

#[tokio::test]
async fn a_message_is_answered_in_the_conversation_it_arrived_in() {
    let mut chat = Chat::start().await;

    chat.says(in_sysadmin(ALICE, "help")).await;

    let (to, reply) = chat.next_delivery().await;
    assert_eq!(to, topic("sysadmin", "incident"));
    assert!(reply.starts_with(HELP_STARTS), "{reply}");
    let mut attempts = chat.stops().await;
    assert_eq!(attempts.try_recv(), Err(TryRecvError::Disconnected));
}

#[tokio::test]
async fn a_page_is_answered_where_it_arrived_and_announced_in_the_announcements_conversation() {
    let mut chat = Chat::start().await;

    chat.says(in_sysadmin(ALICE, "page lichess.org is down"))
        .await;

    let (to, reply) = chat.next_delivery().await;
    assert_eq!(to, topic("sysadmin", "incident"));
    assert!(reply.starts_with(PAGED_STARTS), "{reply}");
    assert_eq!(
        chat.next_delivery().await,
        (on_call_topic(), ANNOUNCEMENT.to_owned())
    );
    let mut attempts = chat.stops().await;
    assert_eq!(attempts.try_recv(), Err(TryRecvError::Disconnected));
}

#[tokio::test]
async fn a_page_sent_in_the_announcements_conversation_is_not_announced_there_again() {
    let mut chat = Chat::start().await;

    chat.says(message(ALICE, on_call_topic(), "page lichess.org is down"))
        .await;

    let (to, reply) = chat.next_delivery().await;
    assert_eq!(to, on_call_topic());
    assert!(reply.starts_with(PAGED_STARTS), "{reply}");
    let mut attempts = chat.stops().await;
    assert_eq!(attempts.try_recv(), Err(TryRecvError::Disconnected));
}

#[tokio::test]
async fn messages_are_still_answered_after_a_delivery_failed() {
    let mut chat = Chat::start().await;
    chat.unreachable.store(true, Ordering::SeqCst);
    chat.says(in_sysadmin(ALICE, "page lichess.org is down"))
        .await;
    let reply = chat.next_attempt().await;
    let announcement = chat.next_attempt().await;
    assert!(matches!(
        reply,
        Attempt::Message {
            delivered: false,
            ..
        }
    ));
    assert_eq!(
        announcement,
        Attempt::Message {
            to: on_call_topic(),
            markdown: ANNOUNCEMENT.to_owned(),
            delivered: false,
        }
    );

    chat.unreachable.store(false, Ordering::SeqCst);
    chat.says(in_sysadmin(ALICE, "help")).await;

    let (to, reply) = chat.next_delivery().await;
    assert_eq!(to, topic("sysadmin", "incident"));
    assert!(reply.starts_with(HELP_STARTS), "{reply}");
}

#[tokio::test]
async fn when_the_messages_end_the_one_in_hand_is_answered_and_then_answering_ends() {
    let chat = Chat::start().await;
    chat.says(in_sysadmin(ALICE, "page lichess.org is down"))
        .await;
    chat.says(in_sysadmin(ALICE, "help")).await;

    let mut attempts = chat.stops().await;

    let mut delivered = Vec::new();
    while let Ok(Attempt::Message { to, .. }) = attempts.try_recv() {
        delivered.push(to);
    }
    assert_eq!(
        delivered,
        [
            topic("sysadmin", "incident"),
            on_call_topic(),
            topic("sysadmin", "incident")
        ]
    );
}

fn wake_me() -> IncomingMessage {
    IncomingMessage {
        id: WAKE_ME,
        ..in_sysadmin(ALICE, "wake me for 4 hours")
    }
}

#[tokio::test]
async fn an_acknowledgement_adds_a_check_mark_to_the_senders_message_and_posts_nothing() {
    let mut chat = Chat::start().await;

    chat.says(wake_me()).await;

    assert_eq!(
        chat.next_attempt().await,
        Attempt::CheckMark {
            on: WAKE_ME,
            added: true
        }
    );
    let mut attempts = chat.stops().await;
    assert_eq!(attempts.try_recv(), Err(TryRecvError::Disconnected));
}

#[tokio::test]
async fn when_the_check_mark_cannot_be_added_the_answer_is_posted_as_a_message_instead() {
    let (logs, _listening) = Logs::listen();
    let mut chat = Chat::start().await;
    chat.refuses_check_marks.store(true, Ordering::SeqCst);

    chat.says(wake_me()).await;

    assert_eq!(
        chat.next_attempt().await,
        Attempt::CheckMark {
            on: WAKE_ME,
            added: false
        }
    );
    assert_eq!(
        chat.next_delivery().await,
        (topic("sysadmin", "incident"), NOW_ON_CALL.to_owned())
    );
    let mut attempts = chat.stops().await;
    assert_eq!(attempts.try_recv(), Err(TryRecvError::Disconnected));
    let logged = logs.text();
    assert!(logged.contains("WARN"), "{logged}");
    assert!(logged.contains("acknowledgement failed"), "{logged}");
    assert!(logged.contains("message_id=77"), "{logged}");
}

#[tokio::test]
async fn a_reply_that_tells_of_others_at_level_one_is_posted_and_no_check_mark_is_added() {
    let dave = user("PDAVE01", "Dave Example");
    let beside_dave = only_policy(&[level(1, &[primary_rota()], &[dave])]);
    let mut chat = Chat::start_with(Setup::with_roster(beside_dave)).await;

    chat.says(wake_me()).await;

    assert_eq!(
        chat.next_delivery().await,
        (
            topic("sysadmin", "incident"),
            NOW_ON_CALL_WITH_DAVE.to_owned()
        )
    );
    let mut attempts = chat.stops().await;
    assert_eq!(attempts.try_recv(), Err(TryRecvError::Disconnected));
}
