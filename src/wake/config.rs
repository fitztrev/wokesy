use std::time::Duration;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(with = "humantime_serde")]
    pub longest: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            longest: Duration::from_hours(7 * 24),
        }
    }
}
