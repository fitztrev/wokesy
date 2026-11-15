use std::time::Duration;

use serde::Deserialize;

use super::chosen_policies::ChosenPolicies;
use crate::chat::{ChannelName, Conversation, TopicName};

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(with = "humantime_serde")]
    pub refresh_interval: Duration,
    pub escalation_policies: ChosenPolicies,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_mins(1),
            escalation_policies: ChosenPolicies::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Announcements {
    pub channel: ChannelName,
    #[serde(default = "on_call_topic")]
    pub topic: TopicName,
}

impl Announcements {
    /// ```
    /// use wokesy::chat::{ChannelName, Conversation, TopicName};
    /// use wokesy::on_call::Announcements;
    ///
    /// let channel = ChannelName::try_new("sysadmin").unwrap();
    /// let topic = TopicName::try_new("on call").unwrap();
    /// let announcements = Announcements {
    ///     channel: channel.clone(),
    ///     topic: topic.clone(),
    /// };
    /// assert_eq!(announcements.conversation(), Conversation::Topic { channel, topic });
    /// ```
    #[must_use]
    pub fn conversation(&self) -> Conversation {
        Conversation::Topic {
            channel: self.channel.clone(),
            topic: self.topic.clone(),
        }
    }
}

#[expect(clippy::expect_used)]
fn on_call_topic() -> TopicName {
    TopicName::try_new("on call").expect("static topic name is valid")
}
