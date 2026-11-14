use std::time::Duration;

use secrecy::SecretString;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

use super::*;
use crate::app::http_client;
use crate::zulip::BotEmail;

const PATIENCE: Duration = Duration::from_secs(5);
const OFTEN: Duration = Duration::from_millis(10);
const ONCE: Duration = Duration::from_hours(1);
const ABILITIES: &str = "/abilities";

struct FakeServices(MockServer);

impl FakeServices {
    async fn start(pagerduty_answers: &[u16]) -> Self {
        let server = MockServer::start().await;
        for status in pagerduty_answers {
            pagerduty_abilities()
                .respond_with(ResponseTemplate::new(*status))
                .up_to_n_times(1)
                .mount(&server)
                .await;
        }
        if let Some(status) = pagerduty_answers.last() {
            pagerduty_abilities()
                .respond_with(ResponseTemplate::new(*status))
                .mount(&server)
                .await;
        }
        let bot = json!({ "result": "success", "msg": "", "user_id": 1, "full_name": "Wokesy" });
        zulip_own_user()
            .respond_with(ResponseTemplate::new(200).set_body_json(bot))
            .mount(&server)
            .await;
        Self(server)
    }

    async fn pagerduty_answers(&self, status: u16) {
        pagerduty_abilities()
            .respond_with(ResponseTemplate::new(status))
            .with_priority(1)
            .mount(&self.0)
            .await;
    }

    async fn pagerduty_answers_after(&self, delay: Duration) {
        pagerduty_abilities()
            .respond_with(ResponseTemplate::new(200).set_delay(delay))
            .with_priority(1)
            .mount(&self.0)
            .await;
    }

    async fn zulip_refuses_the_key(&self) {
        let refusal =
            json!({ "result": "error", "msg": "Invalid API key", "code": "INVALID_API_KEY" });
        zulip_own_user()
            .respond_with(ResponseTemplate::new(401).set_body_json(refusal))
            .with_priority(1)
            .mount(&self.0)
            .await;
    }

    async fn pagerduty_checks(&self) -> usize {
        let requests = self.0.received_requests().await.unwrap();
        requests
            .iter()
            .filter(|request| request.url.path() == ABILITIES)
            .count()
    }

    fn url(&self) -> Url {
        self.0.uri().parse().unwrap()
    }

    fn pagerduty(&self) -> pagerduty::Api {
        pagerduty_at(self.url())
    }

    fn zulip(&self) -> zulip::Api {
        let config = zulip::Config {
            site: self.url(),
            email: BotEmail::try_new("wokesy-bot@zulip.example.com").unwrap(),
            api_key: SecretString::from("zulip_secret"),
            inbox: zulip::InboxConfig::default(),
        };
        zulip::Api::new(http_client().unwrap(), &config)
    }
}

fn pagerduty_abilities() -> MockBuilder {
    Mock::given(method("GET")).and(path(ABILITIES))
}

fn zulip_own_user() -> MockBuilder {
    Mock::given(method("GET")).and(path("/api/v1/users/me"))
}

fn pagerduty_at(base_url: Url) -> pagerduty::Api {
    let config = pagerduty::Config {
        token: SecretString::from("pagerduty_secret"),
        base_url,
        events: None,
    };
    pagerduty::Api::new(http_client().unwrap(), &config)
}

async fn nothing_listening() -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{address}/").parse().unwrap()
}

fn everything_available() -> Health {
    Health {
        pagerduty: Availability::Available,
        zulip: Availability::Available,
    }
}

fn unavailable(recovery: Recovery, error: &str) -> Availability {
    Availability::Unavailable(Unavailable {
        recovery,
        error: error.to_owned(),
    })
}

fn every(interval: Duration) -> Config {
    Config {
        interval,
        timeout: Duration::from_secs(1),
        retries: 2,
        initial_backoff: Duration::from_millis(10),
    }
}

struct Checking {
    health: watch::Receiver<Option<Health>>,
    checking: JoinHandle<()>,
}

impl Checking {
    async fn next(&mut self) -> Health {
        tokio::time::timeout(PATIENCE, self.health.changed())
            .await
            .unwrap()
            .unwrap();
        self.health.borrow_and_update().clone().unwrap()
    }
}

fn checking(config: Config, pagerduty: pagerduty::Api, zulip: zulip::Api) -> Checking {
    let (published, health) = watch::channel(None);
    Checking {
        health,
        checking: tokio::spawn(keep_checking(config, pagerduty, zulip, published)),
    }
}

fn checking_services(services: &FakeServices, config: Config) -> Checking {
    checking(config, services.pagerduty(), services.zulip())
}

#[tokio::test]
async fn when_pagerduty_and_zulip_answer_then_everything_is_available() {
    let services = FakeServices::start(&[200]).await;
    let mut checking = checking_services(&services, every(ONCE));

    assert_eq!(checking.next().await, everything_available());
}

#[tokio::test]
async fn when_pagerduty_fails_once_and_passes_on_the_retry_then_it_is_never_unavailable() {
    let services = FakeServices::start(&[503, 200]).await;
    let mut checking = checking_services(&services, every(ONCE));

    assert_eq!(checking.next().await, everything_available());
    assert_eq!(services.pagerduty_checks().await, 2);
}

#[tokio::test]
async fn when_pagerduty_keeps_failing_through_the_retries_then_it_is_unavailable_for_now() {
    let services = FakeServices::start(&[503]).await;
    let mut checking = checking_services(&services, every(ONCE));

    assert_eq!(
        checking.next().await.pagerduty,
        unavailable(
            Recovery::Recoverable,
            "pagerduty rejected the request (503 Service Unavailable): Service Unavailable",
        )
    );
    assert_eq!(services.pagerduty_checks().await, 3);
}

#[tokio::test]
async fn when_pagerduty_refuses_the_token_then_it_needs_a_person_and_is_not_retried() {
    let services = FakeServices::start(&[401]).await;
    let mut checking = checking_services(&services, every(ONCE));

    assert_eq!(
        checking.next().await.pagerduty,
        unavailable(
            Recovery::NeedsAPerson,
            "pagerduty rejected the request (401 Unauthorized): Unauthorized",
        )
    );
    assert_eq!(services.pagerduty_checks().await, 1);
}

#[tokio::test]
async fn when_pagerduty_asks_for_fewer_requests_then_it_is_retried_and_unavailable_for_now() {
    let services = FakeServices::start(&[429]).await;
    let mut checking = checking_services(&services, every(ONCE));

    assert_eq!(
        checking.next().await.pagerduty,
        unavailable(
            Recovery::Recoverable,
            "pagerduty rejected the request (429 Too Many Requests): Too Many Requests",
        )
    );
    assert_eq!(services.pagerduty_checks().await, 3);
}

#[tokio::test]
async fn when_pagerduty_does_not_answer_within_the_timeout_then_it_is_unavailable_for_now() {
    let services = FakeServices::start(&[200]).await;
    services.pagerduty_answers_after(PATIENCE * 2).await;
    let config = Config {
        timeout: Duration::from_millis(100),
        ..every(ONCE)
    };
    let mut checking = checking_services(&services, config);

    assert_eq!(
        checking.next().await.pagerduty,
        unavailable(Recovery::Recoverable, "no answer within 100ms")
    );
}

#[tokio::test]
async fn when_pagerduty_cannot_be_reached_then_it_is_unavailable_for_now() {
    let services = FakeServices::start(&[200]).await;
    let pagerduty = pagerduty_at(nothing_listening().await);
    let mut checking = checking(every(ONCE), pagerduty, services.zulip());

    let health = checking.next().await;

    assert!(
        matches!(
            &health.pagerduty,
            Availability::Unavailable(Unavailable {
                recovery: Recovery::Recoverable,
                ..
            })
        ),
        "{health:?}"
    );
    assert_eq!(health.zulip, Availability::Available);
}

#[tokio::test]
async fn when_zulip_refuses_the_api_key_then_only_zulip_needs_a_person() {
    let services = FakeServices::start(&[200]).await;
    services.zulip_refuses_the_key().await;
    let mut checking = checking_services(&services, every(ONCE));

    assert_eq!(
        checking.next().await,
        Health {
            pagerduty: Availability::Available,
            zulip: unavailable(
                Recovery::NeedsAPerson,
                "zulip rejected the request (401 Unauthorized): Invalid API key",
            ),
        }
    );
}

#[tokio::test]
async fn while_later_checks_find_the_same_then_listeners_are_not_told_again() {
    let services = FakeServices::start(&[200]).await;
    let mut checking = checking_services(&services, every(OFTEN));
    assert_eq!(checking.next().await, everything_available());

    tokio::time::timeout(PATIENCE, async {
        while services.pagerduty_checks().await < 4 {
            tokio::time::sleep(OFTEN).await;
        }
    })
    .await
    .unwrap();

    assert!(!checking.health.has_changed().unwrap());
}

#[tokio::test]
async fn when_pagerduty_accepts_the_token_again_then_it_is_available_again() {
    let services = FakeServices::start(&[401]).await;
    let mut checking = checking_services(&services, every(OFTEN));
    assert_eq!(
        checking.next().await.pagerduty,
        unavailable(
            Recovery::NeedsAPerson,
            "pagerduty rejected the request (401 Unauthorized): Unauthorized",
        )
    );

    services.pagerduty_answers(200).await;

    assert_eq!(checking.next().await, everything_available());
}

#[tokio::test]
async fn when_nobody_listens_any_more_then_checking_ends() {
    let services = FakeServices::start(&[200]).await;
    let Checking { health, checking } = checking_services(&services, every(OFTEN));

    drop(health);

    tokio::time::timeout(PATIENCE, checking)
        .await
        .unwrap()
        .unwrap();
}
