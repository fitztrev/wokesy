mod config;

use std::fmt;
use std::time::{Duration, SystemTime};

use tokio::sync::watch;
use tokio::time::Instant;
use uuid::Uuid;

use crate::chat::{Conversation, Messenger, OutgoingMessage, TopicName, TopicNameError};
use crate::health::{Availability, Health, Recovery, Unavailable};

pub use config::Config;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InstanceId(Uuid);

impl InstanceId {
    fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// ```
/// use wokesy::status::Signal;
///
/// assert_eq!(Signal::Interrupt.to_string(), "SIGINT");
/// assert_eq!(Signal::Terminate.to_string(), "SIGTERM");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Interrupt,
    Terminate,
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Interrupt => "SIGINT",
            Self::Terminate => "SIGTERM",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shutdown {
    Requested(Signal),
    Failed(String),
}

#[derive(Debug)]
pub struct Status<M> {
    messenger: M,
    instance: InstanceId,
    conversation: Option<Conversation>,
    started: Instant,
    pagerduty: Option<Availability>,
}

impl<M: Messenger> Status<M> {
    pub fn new(messenger: M, config: Option<Config>) -> Result<Self, TopicNameError> {
        let instance = InstanceId::generate();
        let conversation = config
            .map(|config| {
                Ok(Conversation::Topic {
                    channel: config.channel,
                    topic: topic_for(instance, SystemTime::now())?,
                })
            })
            .transpose()?;
        Ok(Self {
            messenger,
            instance,
            conversation,
            started: Instant::now(),
            pagerduty: None,
        })
    }

    pub async fn started(&self) {
        tracing::info!(instance = %self.instance, conversation = ?self.conversation, "wokesy started");
        self.post(format!(
            ":rocket: wokesy v{VERSION} started — [changelog]({REPOSITORY}/releases/tag/v{VERSION})"
        ))
        .await;
    }

    pub async fn follow(&mut self, mut health: watch::Receiver<Option<Health>>) {
        loop {
            let latest = health.borrow_and_update().clone();
            if let Some(Health { pagerduty, .. }) = latest {
                if let Some(change) = change(self.pagerduty.as_ref(), &pagerduty) {
                    self.post(change).await;
                }
                self.pagerduty = Some(pagerduty);
            }
            if health.changed().await.is_err() {
                return;
            }
        }
    }

    pub async fn stopping(&self, shutdown: Shutdown) {
        let uptime = self.uptime();
        self.post(match shutdown {
            Shutdown::Requested(signal) => {
                format!(":octagonal_sign: Shutting down ({signal}) after {uptime}")
            }
            Shutdown::Failed(error) => {
                format!(":octagonal_sign: Shutting down after {uptime}: {error}")
            }
        })
        .await;
    }

    async fn post(&self, markdown: String) {
        let message = OutgoingMessage::new(markdown);
        let Some(conversation) = &self.conversation else {
            tracing::info!(
                instance = %self.instance,
                status = message.markdown(),
                "status not posted because no status channel is configured"
            );
            return;
        };
        if let Err(error) = self.messenger.send(conversation, &message).await {
            tracing::warn!(
                error = %error,
                cause = %error.cause,
                instance = %self.instance,
                ?conversation,
                "status delivery failed"
            );
        }
    }

    fn uptime(&self) -> humantime::FormattedDuration {
        humantime::format_duration(Duration::from_secs(self.started.elapsed().as_secs()))
    }
}

fn change(before: Option<&Availability>, after: &Availability) -> Option<String> {
    match (before, after) {
        (None, Availability::Available) => Some(":check: PagerDuty works".to_owned()),
        (Some(Availability::Unavailable(_)), Availability::Available) => {
            Some(":check: PagerDuty works again".to_owned())
        }
        (Some(Availability::Available), Availability::Available) => None,
        (before, Availability::Unavailable(Unavailable { recovery, error })) => {
            let known = before
                .and_then(Availability::unavailable)
                .map(|before| before.recovery);
            (known != Some(*recovery)).then(|| match recovery {
                Recovery::Recoverable => {
                    format!(":warning: Cannot use PagerDuty, will keep trying: {error}")
                }
                Recovery::NeedsAPerson => format!(
                    ":rotating_light: PagerDuty refuses wokesy, a person must fix it: {error}"
                ),
            })
        }
    }
}

fn topic_for(instance: InstanceId, started: SystemTime) -> Result<TopicName, TopicNameError> {
    let timestamp = humantime::format_rfc3339_seconds(started).to_string();
    let date = timestamp
        .split_once('T')
        .map_or(timestamp.as_str(), |(date, _)| date);
    TopicName::try_new(format!("v{VERSION} {instance} {date}"))
}

#[cfg(test)]
mod tests;
