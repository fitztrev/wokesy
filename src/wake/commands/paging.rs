use std::time::Duration;

use humantime::{FormattedDuration, format_duration};
use time::OffsetDateTime;
use tokio::time::Instant;

use super::{Answer, Commands};
use crate::chat::{ChatUserId, IncomingMessage, OutgoingMessage, silenced};
use crate::on_call::{Roster, on_call_now};
use crate::pagerduty::{ApiError, Events, Summary, SummaryError};
use crate::people::{Directory, Name};
use crate::wake::parsing::Trouble;

const PAGING_IS_OFF: &str = "Paging is not set up here. Use PagerDuty directly.";
const NOTHING_TO_TELL: &str = "What should I tell them? For example: `page lichess.org is down`.";
const PAGED: &str = ":rotating_light: Paged. PagerDuty is alerting whoever is on call.";
const LONGEST_SUMMARY: usize = 1024;
const LINE_ENDINGS: [char; 2] = ['\n', '\r'];

#[derive(Clone, Copy, Debug)]
pub(super) struct Paged {
    by: ChatUserId,
    at: Instant,
}

#[derive(Debug, thiserror::Error)]
enum PageError {
    #[error(transparent)]
    NothingToSay(#[from] SummaryError),
    #[error(transparent)]
    Refused(#[from] ApiError),
}

impl Commands {
    pub(super) async fn page(
        &mut self,
        message: &IncomingMessage,
        trouble: Option<Trouble>,
        read: Option<&(Roster, Directory)>,
    ) -> Answer {
        let Some(events) = &self.events else {
            return Answer::only(PAGING_IS_OFF);
        };
        let Some(trouble) = trouble else {
            return Answer::only(NOTHING_TO_TELL);
        };
        if let Some(refusal) = self.too_soon_to_page_again() {
            return Answer::only(refusal);
        }
        if let Err(error) = alert(events, &message.sender_name, &trouble).await {
            tracing::error!(sender = message.sender.0, error = %error, cause = ?std::error::Error::source(&error), "could not page");
            return Answer::only(format!(
                ":warning: I couldn't page: {error}. Use PagerDuty directly."
            ));
        }
        self.last_page = Some(Paged {
            by: message.sender,
            at: Instant::now(),
        });
        tracing::info!(sender = message.sender.0, "paged whoever is on call");
        Answer {
            reply: OutgoingMessage::new(paged(read)),
            announcement: Some(OutgoingMessage::new(announcement(message.sender, &trouble))),
        }
    }

    fn too_soon_to_page_again(&self) -> Option<String> {
        let last = self.last_page?;
        let elapsed = last.at.elapsed();
        let remaining = self.config.page_cooldown.saturating_sub(elapsed);
        (!remaining.is_zero()).then(|| {
            format!(
                ":hourglass: {} already paged {} ago. I won't page again for another {}. PagerDuty keeps escalating that alert until someone acknowledges it.",
                last.by.silent_mention(),
                seconds(elapsed.as_secs()),
                seconds(seconds_begun(remaining)),
            )
        })
    }
}

async fn alert(events: &Events, sender: &Name, trouble: &Trouble) -> Result<(), PageError> {
    let summary: String = format!("{}: {trouble}", sender.as_str())
        .chars()
        .take(LONGEST_SUMMARY)
        .collect();
    events.trigger(&Summary::try_new(summary)?, sender).await?;
    Ok(())
}

fn seconds(seconds: u64) -> FormattedDuration {
    format_duration(Duration::from_secs(seconds))
}

fn seconds_begun(remaining: Duration) -> u64 {
    remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() > 0))
}

fn paged(read: Option<&(Roster, Directory)>) -> String {
    match read {
        Some((roster, directory)) => format!(
            "{PAGED}\n\n{}",
            on_call_now(roster, directory, OffsetDateTime::now_utc())
        ),
        None => PAGED.to_owned(),
    }
}

fn announcement(sender: ChatUserId, trouble: &Trouble) -> String {
    let quoted: Vec<String> = silenced(trouble)
        .replace("\r\n", "\n")
        .split(LINE_ENDINGS)
        .map(|line| format!("> {line}"))
        .collect();
    format!(
        ":rotating_light: {} paged on call:\n{}",
        sender.silent_mention(),
        quoted.join("\n")
    )
}

#[cfg(test)]
mod tests;
