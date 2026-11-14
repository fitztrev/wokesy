use std::time::Duration;

use serde::Deserialize;

use super::channels::Channels;
use crate::chat::ChannelName;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(with = "humantime_serde")]
    pub longest: Duration,
    pub channels: Channels,
    #[serde(with = "humantime_serde")]
    pub page_cooldown: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            longest: Duration::from_hours(7 * 24),
            channels: Channels::default(),
            page_cooldown: Duration::from_mins(5),
        }
    }
}

impl Config {
    /// ```
    /// use wokesy::chat::ChannelName;
    /// use wokesy::wake::Config;
    ///
    /// let channel = |name: &str| ChannelName::try_new(name).unwrap();
    /// let announcements = channel("announcements");
    ///
    /// let unset = Config::default();
    /// assert_eq!(unset.open_channels(&announcements), [announcements.clone()]);
    ///
    /// let set = Config {
    ///     channels: "sysadmin, on-call".into(),
    ///     ..Config::default()
    /// };
    /// assert_eq!(set.open_channels(&announcements), [channel("sysadmin"), channel("on-call")]);
    /// ```
    #[must_use]
    pub fn open_channels(&self, announcements: &ChannelName) -> Vec<ChannelName> {
        match self.channels.names() {
            [] => vec![announcements.clone()],
            channels => channels.to_vec(),
        }
    }
}
