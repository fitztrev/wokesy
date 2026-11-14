use tokio::sync::mpsc;

use super::commands::Commands;
use crate::chat::{Conversation, IncomingMessage, Messenger, OutgoingMessage};

pub async fn keep_answering<M: Messenger>(
    mut commands: Commands,
    messenger: M,
    announcements: Conversation,
    mut incoming: mpsc::Receiver<IncomingMessage>,
) {
    while let Some(message) = incoming.recv().await {
        let answer = commands.answer(&message).await;
        deliver(&messenger, &message.reply_to, &answer.reply).await;
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
