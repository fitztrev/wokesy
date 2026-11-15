use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinHandle;

use super::*;
use crate::chat::DeliveryError;
use crate::wake::testing::{ALICE, Setup, in_sysadmin, message, topic};

const PATIENCE: Duration = Duration::from_secs(5);
const HELP_STARTS: &str = "I manage who is on call in PagerDuty.";
const PAGED_STARTS: &str = ":rotating_light: Paged @_**|13**.";
const ANNOUNCEMENT: &str = ":rotating_light: @_**|12** paged on call:\n> lichess.org is down";

#[derive(Clone, Debug)]
struct RecordingMessenger {
    attempts: mpsc::UnboundedSender<Attempt>,
    unreachable: Arc<AtomicBool>,
}

#[derive(Debug, PartialEq, Eq)]
struct Attempt {
    to: Conversation,
    markdown: String,
    delivered: bool,
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
            .send(Attempt {
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

struct Chat {
    task: JoinHandle<()>,
    incoming: mpsc::Sender<IncomingMessage>,
    attempts: mpsc::UnboundedReceiver<Attempt>,
    unreachable: Arc<AtomicBool>,
}

fn on_call_topic() -> Conversation {
    topic("sysadmin", "on call")
}

impl Chat {
    async fn start() -> Self {
        let answering = Setup::default().start().await;
        answering.accepts_pages().await;
        let (attempted, attempts) = mpsc::unbounded_channel();
        let unreachable = Arc::new(AtomicBool::new(false));
        let messenger = RecordingMessenger {
            attempts: attempted,
            unreachable: Arc::clone(&unreachable),
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
        let attempt = self.next_attempt().await;
        assert!(attempt.delivered, "{attempt:?}");
        (attempt.to, attempt.markdown)
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
    assert!(!reply.delivered && !announcement.delivered);
    assert_eq!(announcement.to, on_call_topic());

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
    while let Ok(attempt) = attempts.try_recv() {
        delivered.push(attempt.to);
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
