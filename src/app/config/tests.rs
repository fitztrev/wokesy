use std::time::Duration;

use figment::Jail;
use secrecy::ExposeSecret;

use super::*;
use crate::app::telemetry::LogFormat;
use crate::chat::ChatUserId;
use crate::people::{Email, Links};

#[expect(clippy::result_large_err)]
fn in_clean_environment(test: impl FnOnce(&mut Jail)) {
    Jail::expect_with(|jail| {
        jail.clear_env();
        test(jail);
        Ok(())
    });
}

const REQUIRED: [(&str, &str); 5] = [
    ("WOKESY_ZULIP__SITE", "https://chat.example.com"),
    ("WOKESY_ZULIP__EMAIL", "wokesy-bot@chat.example.com"),
    ("WOKESY_ZULIP__API_KEY", "secret"),
    ("WOKESY_PAGERDUTY__TOKEN", "secret"),
    ("WOKESY_ANNOUNCEMENTS__CHANNEL", "sysadmin"),
];

fn load(jail: &mut Jail) -> Result<Config, Box<figment::Error>> {
    for (variable, value) in REQUIRED {
        jail.set_env(variable, value);
    }
    Config::from_env()
}

#[test]
fn with_only_the_required_variables_set_every_other_setting_takes_its_default() {
    in_clean_environment(|jail| {
        let config = load(jail).unwrap();
        assert!(config.http.host.is_loopback());
        assert_eq!(config.http.port, 8080);
        assert_eq!(config.zulip.inbox.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.zulip.inbox.max_backoff, Duration::from_mins(1));
        assert_eq!(config.announcements.topic.as_str(), "on call");
        assert_eq!(config.on_call.refresh_interval, Duration::from_mins(1));
        assert_eq!(config.people.links, Links::default());
        assert_eq!(config.people.refresh_interval, Duration::from_mins(5));
        assert_eq!(config.wake.longest, Duration::from_hours(7 * 24));
        assert!(config.status.is_none());
        assert_eq!(config.health.interval, Duration::from_secs(30));
        assert_eq!(config.health.timeout, Duration::from_secs(5));
        assert_eq!(config.health.retries, 2);
        assert_eq!(config.health.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.telemetry.log_format, LogFormat::Json);
    });
}

#[test]
fn loading_fails_when_any_required_variable_is_missing() {
    for (missing, _) in REQUIRED {
        in_clean_environment(|jail| {
            for (variable, value) in REQUIRED {
                if variable != missing {
                    jail.set_env(variable, value);
                }
            }
            assert!(Config::from_env().is_err(), "{missing} was not required");
        });
    }
}

#[test]
fn every_variable_a_deployment_can_set_is_recognized() {
    in_clean_environment(|jail| {
        jail.set_env("WOKESY_ZULIP__INBOX__INITIAL_BACKOFF", "2s");
        jail.set_env("WOKESY_ZULIP__INBOX__MAX_BACKOFF", "2m");
        jail.set_env(
            "WOKESY_PAGERDUTY__BASE_URL",
            "https://api.eu.pagerduty.com/",
        );
        jail.set_env("WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY", "secret");
        jail.set_env(
            "WOKESY_PAGERDUTY__EVENTS__URL",
            "https://events.eu.pagerduty.com/",
        );
        jail.set_env("WOKESY_ANNOUNCEMENTS__TOPIC", "who is on call");
        jail.set_env("WOKESY_ON_CALL__REFRESH_INTERVAL", "30s");
        jail.set_env("WOKESY_PEOPLE__LINKS", "alice@example.com=12");
        jail.set_env("WOKESY_PEOPLE__REFRESH_INTERVAL", "10m");
        jail.set_env("WOKESY_WAKE__LONGEST", "2d");
        jail.set_env("WOKESY_STATUS__CHANNEL", "wokesy-status");
        jail.set_env("WOKESY_HEALTH__INTERVAL", "1m");
        jail.set_env("WOKESY_HEALTH__TIMEOUT", "10s");
        jail.set_env("WOKESY_HEALTH__RETRIES", "3");
        jail.set_env("WOKESY_HEALTH__INITIAL_BACKOFF", "2s");
        jail.set_env("WOKESY_HTTP__HOST", "0.0.0.0");
        jail.set_env("WOKESY_HTTP__PORT", "9090");
        jail.set_env("WOKESY_TELEMETRY__LOG_FORMAT", "pretty");
        let config = load(jail).unwrap();
        assert_eq!(config.zulip.inbox.initial_backoff, Duration::from_secs(2));
        assert_eq!(config.zulip.inbox.max_backoff, Duration::from_mins(2));
        assert_eq!(config.announcements.topic.as_str(), "who is on call");
        assert_eq!(config.on_call.refresh_interval, Duration::from_secs(30));
        assert_eq!(config.people.refresh_interval, Duration::from_mins(10));
        assert_eq!(config.wake.longest, Duration::from_hours(2 * 24));
        assert_eq!(config.status.unwrap().channel.as_str(), "wokesy-status");
        assert_eq!(config.health.interval, Duration::from_mins(1));
        assert_eq!(config.health.timeout, Duration::from_secs(10));
        assert_eq!(config.health.retries, 3);
        assert_eq!(config.health.initial_backoff, Duration::from_secs(2));
        assert!(config.http.host.is_unspecified());
        assert_eq!(config.http.port, 9090);
        assert_eq!(config.telemetry.log_format, LogFormat::Pretty);
    });
}

#[test]
fn a_misspelled_variable_in_any_section_is_rejected() {
    for misspelled in [
        "WOKESY_HTTPS__PORT",
        "WOKESY_HTTP__PROT",
        "WOKESY_ZULIP__API_KEE",
        "WOKESY_ZULIP__INBOX__MAX_BACKOF",
        "WOKESY_PAGERDUTY__TOKN",
        "WOKESY_PAGERDUTY__EVENTS__ROUTING_KEE",
        "WOKESY_ANNOUNCEMENTS__TOPIK",
        "WOKESY_ON_CALL__REFRESH",
        "WOKESY_PEOPLE__LINK",
        "WOKESY_WAKE__LONGST",
        "WOKESY_STATUS__CHANEL",
        "WOKESY_HEALTH__RETRYS",
        "WOKESY_TELEMETRY__FORMAT",
    ] {
        in_clean_environment(|jail| {
            jail.set_env("WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY", "secret");
            jail.set_env("WOKESY_STATUS__CHANNEL", "wokesy-status");
            load(jail).unwrap();
            jail.set_env(misspelled, "1");
            assert!(Config::from_env().is_err(), "{misspelled} was accepted");
        });
    }
}

#[test]
fn the_status_topic_cannot_be_configured_because_each_instance_names_its_own() {
    in_clean_environment(|jail| {
        jail.set_env("WOKESY_STATUS__CHANNEL", "wokesy-status");
        jail.set_env("WOKESY_STATUS__TOPIC", "wokesy");
        assert!(load(jail).is_err(), "WOKESY_STATUS__TOPIC was accepted");
    });
}

#[test]
fn pagerduty_is_read_at_its_us_address_unless_configured_otherwise() {
    in_clean_environment(|jail| {
        assert_eq!(
            load(jail).unwrap().pagerduty.base_url.as_str(),
            "https://api.pagerduty.com/"
        );
        jail.set_env(
            "WOKESY_PAGERDUTY__BASE_URL",
            "https://api.eu.pagerduty.com/",
        );
        assert_eq!(
            load(jail).unwrap().pagerduty.base_url.as_str(),
            "https://api.eu.pagerduty.com/"
        );
    });
}

#[test]
fn without_an_events_variable_there_is_no_events_section() {
    in_clean_environment(|jail| {
        assert!(load(jail).unwrap().pagerduty.events.is_none());
    });
}

#[test]
fn with_only_the_routing_key_set_events_go_to_pagerduty_at_its_us_address() {
    in_clean_environment(|jail| {
        jail.set_env("WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY", "routing-secret");
        let events = load(jail).unwrap().pagerduty.events.unwrap();
        assert_eq!(events.routing_key.expose_secret(), "routing-secret");
        assert_eq!(events.url.as_str(), "https://events.pagerduty.com/");
    });
}

#[test]
fn an_events_address_without_a_routing_key_is_rejected() {
    in_clean_environment(|jail| {
        jail.set_env(
            "WOKESY_PAGERDUTY__EVENTS__URL",
            "https://events.eu.pagerduty.com/",
        );
        assert!(load(jail).is_err());
    });
}

#[test]
fn a_misspelled_events_variable_is_rejected() {
    in_clean_environment(|jail| {
        jail.set_env("WOKESY_PAGERDUTY__EVENTS__ROUTING_KEY", "secret");
        jail.set_env(
            "WOKESY_PAGERDUTY__EVENTS__URI",
            "https://events.eu.pagerduty.com/",
        );
        assert!(
            load(jail).is_err(),
            "WOKESY_PAGERDUTY__EVENTS__URI was accepted"
        );
    });
}

#[test]
fn links_are_read_with_or_without_whitespace_between_the_pairs() {
    let alice = Email::try_new("alice@example.com").unwrap();
    let bob = Email::try_new("bob@example.com").unwrap();
    for links in [
        "alice@example.com=12, bob@example.com=34",
        "alice@example.com=12,bob@example.com=34",
    ] {
        in_clean_environment(|jail| {
            jail.set_env("WOKESY_PEOPLE__LINKS", links);
            let links = load(jail).unwrap().people.links;
            assert_eq!(links.chat_user_of(&alice), Some(ChatUserId(12)));
            assert_eq!(links.chat_user_of(&bob), Some(ChatUserId(34)));
        });
    }
}

#[test]
fn a_malformed_link_fails_loading() {
    for links in ["alice@example.com=12, bob", "alice@example.com", "12"] {
        in_clean_environment(|jail| {
            jail.set_env("WOKESY_PEOPLE__LINKS", links);
            assert!(load(jail).is_err(), "{links} was accepted");
        });
    }
}
