use std::time::Duration;

use serde::Deserialize;

use super::links::Links;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub links: Links,
    #[serde(with = "humantime_serde")]
    pub refresh_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            links: Links::default(),
            refresh_interval: Duration::from_mins(5),
        }
    }
}
