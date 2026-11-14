use serde::Deserialize;

use crate::chat::ChannelName;

const BETWEEN_CHANNELS: char = ',';

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub struct Channels(Vec<ChannelName>);

impl Channels {
    pub(super) fn names(&self) -> &[ChannelName] {
        &self.0
    }
}

impl From<&str> for Channels {
    fn from(channels: &str) -> Self {
        Self(
            channels
                .split(BETWEEN_CHANNELS)
                .filter_map(|channel| ChannelName::try_new(channel).ok())
                .collect(),
        )
    }
}

impl From<String> for Channels {
    fn from(channels: String) -> Self {
        channels.as_str().into()
    }
}

#[cfg(test)]
mod tests;
