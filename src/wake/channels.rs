use std::str::FromStr;

use serde::Deserialize;

use crate::chat::ChannelName;

const BETWEEN_CHANNELS: char = ',';

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Channels(Vec<ChannelName>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("channel {position} is not a channel name")]
pub struct InvalidChannel {
    pub position: usize,
}

impl Channels {
    pub(super) fn names(&self) -> &[ChannelName] {
        &self.0
    }
}

impl FromStr for Channels {
    type Err = InvalidChannel;

    fn from_str(channels: &str) -> Result<Self, Self::Err> {
        channels
            .split(BETWEEN_CHANNELS)
            .enumerate()
            .filter(|(_, channel)| !channel.trim().is_empty())
            .map(|(index, channel)| {
                ChannelName::try_new(channel).map_err(|_| InvalidChannel {
                    position: index + 1,
                })
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

impl TryFrom<String> for Channels {
    type Error = InvalidChannel;

    fn try_from(channels: String) -> Result<Self, Self::Error> {
        channels.parse()
    }
}

#[cfg(test)]
mod tests;
