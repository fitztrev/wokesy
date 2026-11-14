use secrecy::SecretString;
use serde::Deserialize;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub token: SecretString,
    #[serde(default = "rest_api")]
    pub base_url: Url,
    pub events: Option<EventsConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsConfig {
    pub routing_key: SecretString,
    #[serde(default = "events_api")]
    pub url: Url,
}

#[expect(clippy::expect_used)]
fn rest_api() -> Url {
    Url::parse("https://api.pagerduty.com/").expect("static url is valid")
}

#[expect(clippy::expect_used)]
fn events_api() -> Url {
    Url::parse("https://events.pagerduty.com/").expect("static url is valid")
}
