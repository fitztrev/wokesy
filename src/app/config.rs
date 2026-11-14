use figment::Figment;
use figment::providers::Env;
use serde::Deserialize;

use super::{http, telemetry};
use crate::{health, on_call, pagerduty, people, status, wake, zulip};

pub const ENV_PREFIX: &str = "WOKESY_";
pub const ENV_NESTING_SEPARATOR: &str = "__";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub http: http::Config,
    pub zulip: zulip::Config,
    pub pagerduty: pagerduty::Config,
    pub announcements: on_call::Announcements,
    #[serde(default)]
    pub on_call: on_call::Config,
    #[serde(default)]
    pub people: people::Config,
    #[serde(default)]
    pub wake: wake::Config,
    pub status: Option<status::Config>,
    #[serde(default)]
    pub health: health::Config,
    #[serde(default)]
    pub telemetry: telemetry::Config,
}

impl Config {
    #[must_use]
    pub fn figment() -> Figment {
        Figment::from(Env::prefixed(ENV_PREFIX).split(ENV_NESTING_SEPARATOR))
    }

    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Self::figment().extract().map_err(Box::new)
    }
}

#[cfg(test)]
mod tests;
