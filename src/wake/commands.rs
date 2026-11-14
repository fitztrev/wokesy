mod own_overrides;
mod paging;
mod waking;

use std::sync::Arc;

use tokio::sync::{Notify, watch};

use super::config::Config;
use super::parsing::{Command, parse};
use crate::chat::{ChannelName, Conversation, IncomingMessage, OutgoingMessage};
use crate::on_call::{Roster, on_call_now};
use crate::pagerduty;
use crate::people::Directory;

use own_overrides::OwnOverride;
use paging::Paged;

const STILL_READING: &str = "I'm still reading PagerDuty. Try again in a minute.";
const UNKNOWN_SENDER: &str = "I don't know which PagerDuty user you are. I match people by email address, then by full name. Ask whoever runs wokesy to link your accounts.";
const UNRECOGNIZED: &str = "I don't know that command. Send `help` to see the commands I know.";
const HELP: &str = "\
I manage who is on call in PagerDuty.
- `page lichess.org is down` alerts whoever is on call, through PagerDuty. Anyone here can use it. After a page I wait a while before I page again.
- `wake me for 4 hours` puts you first in line on every escalation policy for that long. PagerDuty hands it back when the time is up.
- `stop waking me` ends that early.
- `back to defaults` ends every override in effect, yours or anyone's, so the schedules decide again.
- `who is on call` shows who is on call now.";
const BETWEEN_OPEN_CHANNELS: &str = " or ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub reply: OutgoingMessage,
    pub announcement: Option<OutgoingMessage>,
}

#[derive(Debug)]
pub struct Commands {
    pagerduty: pagerduty::Api,
    events: Option<pagerduty::Events>,
    roster: watch::Receiver<Option<Roster>>,
    directory: watch::Receiver<Option<Directory>>,
    refresh: Arc<Notify>,
    config: Config,
    open_channels: Vec<ChannelName>,
    last_page: Option<Paged>,
    own_overrides: Vec<OwnOverride>,
}

impl Answer {
    fn only(reply: impl Into<String>) -> Self {
        Self {
            reply: OutgoingMessage::new(reply.into()),
            announcement: None,
        }
    }
}

impl Commands {
    #[must_use]
    pub const fn new(
        pagerduty: pagerduty::Api,
        events: Option<pagerduty::Events>,
        roster: watch::Receiver<Option<Roster>>,
        directory: watch::Receiver<Option<Directory>>,
        refresh: Arc<Notify>,
        config: Config,
        open_channels: Vec<ChannelName>,
    ) -> Self {
        Self {
            pagerduty,
            events,
            roster,
            directory,
            refresh,
            config,
            open_channels,
            last_page: None,
            own_overrides: Vec::new(),
        }
    }

    /// ```
    /// use std::sync::Arc;
    ///
    /// use tokio::sync::{Notify, watch};
    /// use wokesy::chat::{
    ///     ChannelName, ChatUserId, CommandText, Conversation, IncomingMessage, MessageId,
    /// };
    /// use wokesy::pagerduty;
    /// use wokesy::people::Name;
    /// use wokesy::wake::{Commands, Config};
    ///
    /// let pagerduty = pagerduty::Config {
    ///     token: "secret".into(),
    ///     base_url: "https://api.pagerduty.com/".parse().unwrap(),
    ///     events: None,
    /// };
    /// let (_roster, roster) = watch::channel(None);
    /// let (_directory, directory) = watch::channel(None);
    /// let config = Config::default();
    /// let open_channels = config.open_channels(&ChannelName::try_new("sysadmin").unwrap());
    /// let mut commands = Commands::new(
    ///     pagerduty::Api::new(reqwest::Client::new(), &pagerduty),
    ///     None,
    ///     roster,
    ///     directory,
    ///     Arc::new(Notify::new()),
    ///     config,
    ///     open_channels,
    /// );
    /// let sender = ChatUserId(12);
    /// let in_private = IncomingMessage {
    ///     id: MessageId(1),
    ///     sender,
    ///     sender_name: Name::try_new("Alice Example").unwrap(),
    ///     reply_to: Conversation::Direct { user_id: sender },
    ///     text: CommandText::new("who is on call"),
    /// };
    ///
    /// let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    /// let answer = runtime.block_on(commands.answer(&in_private));
    ///
    /// assert_eq!(
    ///     answer.reply.markdown(),
    ///     "I only take commands in the open, in #**sysadmin**. Mention me there."
    /// );
    /// assert_eq!(answer.announcement, None);
    /// ```
    pub async fn answer(&mut self, message: &IncomingMessage) -> Answer {
        if !self.takes_commands_in(&message.reply_to) {
            return Answer::only(self.only_in_the_open());
        }
        let reply = match (parse(&message.text), self.read()) {
            (Command::Page(trouble), read) => {
                return self.page(message, trouble, read.as_ref()).await;
            }
            (Command::Help, _) => HELP.to_owned(),
            (Command::Unrecognized, _) => UNRECOGNIZED.to_owned(),
            (Command::Wake(_) | Command::Stop | Command::Reset | Command::Who, None) => {
                STILL_READING.to_owned()
            }
            (Command::Wake(lasting), Some((roster, directory))) => {
                self.wake(message.sender, lasting, &roster, &directory)
                    .await
            }
            (Command::Stop, Some((_, directory))) => self.stop(message.sender, &directory).await,
            (Command::Reset, Some((_, directory))) => self.reset(message.sender, &directory).await,
            (Command::Who, Some((roster, directory))) => on_call_now(&roster, &directory),
        };
        Answer::only(reply)
    }

    fn takes_commands_in(&self, conversation: &Conversation) -> bool {
        match conversation {
            Conversation::Direct { .. } => false,
            Conversation::Topic { channel, .. } => self.open_channels.contains(channel),
        }
    }

    fn only_in_the_open(&self) -> String {
        let channels: Vec<String> = self
            .open_channels
            .iter()
            .map(|channel| format!("#**{}**", channel.as_str()))
            .collect();
        format!(
            "I only take commands in the open, in {}. Mention me there.",
            channels.join(BETWEEN_OPEN_CHANNELS)
        )
    }

    fn read(&self) -> Option<(Roster, Directory)> {
        let roster = self.roster.borrow().clone()?;
        let directory = self.directory.borrow().clone()?;
        Some((roster, directory))
    }
}

#[cfg(test)]
mod tests;
