use backon::{BackoffBuilder, ExponentialBackoff, ExponentialBuilder};
use tokio::sync::mpsc;

use crate::chat::{CommandText, Conversation, IncomingMessage};

use super::InboxConfig;
use super::api::{Api, ApiError, Bot, Delivery, Flag, Place};

pub async fn keep_reading(api: Api, config: InboxConfig, incoming: mpsc::Sender<IncomingMessage>) {
    tokio::select! {
        () = incoming.closed() => {}
        () = read(&api, &config, &incoming) => {}
    }
}

struct Retry {
    delays: ExponentialBuilder,
    upcoming: ExponentialBackoff,
    max_backoff: std::time::Duration,
}

impl Retry {
    fn new(config: &InboxConfig) -> Self {
        let delays = ExponentialBuilder::new()
            .with_min_delay(config.initial_backoff)
            .with_max_delay(config.max_backoff)
            .without_max_times();
        Self {
            delays,
            upcoming: delays.build(),
            max_backoff: config.max_backoff,
        }
    }

    fn reset(&mut self) {
        self.upcoming = self.delays.build();
    }

    async fn wait(&mut self, error: &ApiError, doing: &'static str) {
        let delay = self.upcoming.next().unwrap_or(self.max_backoff);
        tracing::warn!(error = %error, cause = ?std::error::Error::source(error), retry_in = ?delay, doing, "zulip request failed");
        tokio::time::sleep(delay).await;
    }
}

async fn read(api: &Api, config: &InboxConfig, incoming: &mpsc::Sender<IncomingMessage>) {
    let mut retry = Retry::new(config);
    let bot = loop {
        match api.own_user().await {
            Ok(bot) => break bot,
            Err(error) => retry.wait(&error, "looking up the bot user").await,
        }
    };
    retry.reset();
    loop {
        let mut queue = match api.register().await {
            Ok(queue) => queue,
            Err(error) => {
                retry.wait(&error, "registering an event queue").await;
                continue;
            }
        };
        tracing::info!(queue_id = %queue.id, "zulip event queue registered");
        let mut delivered = false;
        loop {
            match api.events(&queue).await {
                Ok(batch) => {
                    retry.reset();
                    delivered = true;
                    queue.acknowledge(&batch);
                    for delivery in batch.deliveries {
                        if let Some(message) = addressed(&bot, delivery)
                            && incoming.send(message).await.is_err()
                        {
                            return;
                        }
                    }
                }
                Err(error @ ApiError::BadEventQueue) => {
                    if delivered {
                        tracing::info!(queue_id = %queue.id, "zulip event queue expired");
                    } else {
                        retry.wait(&error, "reading a new event queue").await;
                    }
                    break;
                }
                Err(error) => retry.wait(&error, "reading events").await,
            }
        }
    }
}

fn addressed(bot: &Bot, Delivery { message, flags }: Delivery) -> Option<IncomingMessage> {
    let sender = message.sender_id;
    if sender == bot.id {
        return None;
    }
    let (reply_to, text) = match message.place {
        Place::Private { display_recipient } => display_recipient
            .iter()
            .all(|recipient| recipient.id == sender || recipient.id == bot.id)
            .then(|| {
                (
                    Conversation::Direct { user_id: sender },
                    CommandText::new(message.content.as_str()),
                )
            }),
        Place::Stream {
            display_recipient,
            subject,
        } => flags
            .contains(&Flag::Mentioned)
            .then(|| after_own_mention(bot, &message.content))
            .flatten()
            .map(|command| {
                (
                    Conversation::Topic {
                        channel: display_recipient,
                        topic: subject,
                    },
                    CommandText::new(command),
                )
            }),
    }?;
    Some(IncomingMessage {
        id: message.id,
        sender,
        sender_name: message.sender_full_name,
        reply_to,
        text,
    })
}

fn after_own_mention<'a>(bot: &Bot, content: &'a str) -> Option<&'a str> {
    let content = content.trim_start();
    let name = bot.full_name.as_str();
    let mention = format!("@**{name}**");
    let mention_with_id = format!("@**{name}|{}**", bot.id.0);
    content
        .strip_prefix(mention.as_str())
        .or_else(|| content.strip_prefix(mention_with_id.as_str()))
}

#[cfg(test)]
mod tests;
