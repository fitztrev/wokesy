use tokio::sync::mpsc;

use super::commands::{Commands, Reply};
use crate::chat::{Acknowledger, Conversation, IncomingMessage, Messenger, OutgoingMessage};

pub async fn keep_answering<M: Messenger + Acknowledger>(
    mut commands: Commands,
    messenger: M,
    announcements: Conversation,
    mut incoming: mpsc::Receiver<IncomingMessage>,
) {
    while let Some(message) = incoming.recv().await {
        let answer = commands.answer(&message).await;
        match &answer.reply {
            Reply::Message(reply) => deliver(&messenger, &message.reply_to, reply).await,
            Reply::Acknowledgement { otherwise } => {
                if let Err(error) = messenger.acknowledge(message.id).await {
                    tracing::warn!(message_id = message.id.0, error = %error, cause = %error.cause, "acknowledgement failed, so the answer is sent as a message");
                    deliver(&messenger, &message.reply_to, otherwise).await;
                }
            }
        }
        if let Some(announcement) = &answer.announcement
            && message.reply_to != announcements
        {
            deliver(&messenger, &announcements, announcement).await;
        }
    }
}

async fn deliver<M: Messenger>(messenger: &M, to: &Conversation, message: &OutgoingMessage) {
    if let Err(error) = messenger.send(to, message).await {
        tracing::warn!(error = %error, cause = %error.cause, "answer delivery failed");
    }
}

#[cfg(test)]
mod tests;
