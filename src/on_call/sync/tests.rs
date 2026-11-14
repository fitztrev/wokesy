use secrecy::SecretString;
use serde_json::{Value, json};
use time::macros::datetime;
use tokio::task::JoinHandle;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::on_call::testing::{policy_id, policy_name, schedule, shift, user};
use crate::on_call::{Level, Policy, Shift};
use crate::pagerduty::EscalationLevel;

const PATIENCE: Duration = Duration::from_secs(5);
const RARELY: Duration = Duration::from_hours(1);
const OFTEN: Duration = Duration::from_millis(20);
const ESCALATION_POLICIES: &str = "/escalation_policies";
const ONCALLS: &str = "/oncalls";

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn lichess_and_an_unused_policy() -> ResponseTemplate {
    ok(json!({
        "escalation_policies": [
            {
                "id": "PUNUSED",
                "name": "Abandoned",
                "escalation_rules": [{
                    "targets": [{ "id": "PI7DH89", "type": "schedule_reference", "summary": "Forgotten rota" }],
                }],
                "services": [],
            },
            {
                "id": "PLICHES",
                "name": "Lichess",
                "escalation_rules": [
                    { "targets": [{ "id": "PI7DH85", "type": "schedule_reference", "summary": "Primary rota" }] },
                    { "targets": [{ "id": "PCAROL1", "type": "user_reference", "summary": "Carol Example" }] },
                ],
                "services": [{ "id": "PIJ90N7", "type": "service_reference", "summary": "lichess.org" }],
            },
        ],
        "more": false,
    }))
}

fn first_line(id: &str, name: &str, end: &str) -> Value {
    json!({
        "escalation_policy": { "id": "PLICHES", "type": "escalation_policy_reference", "summary": "Lichess" },
        "escalation_level": 1,
        "schedule": { "id": "PI7DH85", "type": "schedule_reference", "summary": "Primary rota" },
        "user": { "id": id, "type": "user_reference", "summary": name },
        "start": "2026-11-14T08:00:00Z",
        "end": end,
    })
}

fn carol_at_the_second_line() -> Value {
    json!({
        "escalation_policy": { "id": "PLICHES", "type": "escalation_policy_reference", "summary": "Lichess" },
        "escalation_level": 2,
        "schedule": null,
        "user": { "id": "PCAROL1", "type": "user_reference", "summary": "Carol Example" },
        "start": null,
        "end": null,
    })
}

fn on_call(first: &Value) -> ResponseTemplate {
    ok(json!({ "oncalls": [carol_at_the_second_line(), first], "more": false }))
}

fn alice_until_the_evening() -> ResponseTemplate {
    on_call(&first_line(
        "PALICE1",
        "Alice Example",
        "2026-11-14T20:00:00Z",
    ))
}

fn alice_until_midnight() -> ResponseTemplate {
    on_call(&first_line(
        "PALICE1",
        "Alice Example",
        "2026-11-15T00:00:00Z",
    ))
}

fn bob_until_midnight() -> ResponseTemplate {
    on_call(&first_line(
        "PBOB001",
        "Bob Example",
        "2026-11-15T00:00:00Z",
    ))
}

fn lichess_with_first(first: Shift) -> Roster {
    Roster {
        policies: vec![Policy {
            id: policy_id("PLICHES"),
            name: policy_name("Lichess"),
            levels: vec![
                Level {
                    number: EscalationLevel::new(1),
                    schedules: vec![schedule("PI7DH85", "Primary rota")],
                    people: vec![],
                    shifts: vec![first],
                },
                Level {
                    number: EscalationLevel::new(2),
                    schedules: vec![],
                    people: vec![user("PCAROL1", "Carol Example")],
                    shifts: vec![shift(user("PCAROL1", "Carol Example"), None)],
                },
            ],
        }],
    }
}

fn with_alice_until_the_evening() -> Roster {
    lichess_with_first(shift(
        user("PALICE1", "Alice Example"),
        Some(datetime!(2026-11-14 20:00 UTC)),
    ))
}

fn with_alice_until_midnight() -> Roster {
    lichess_with_first(shift(
        user("PALICE1", "Alice Example"),
        Some(datetime!(2026-11-15 00:00 UTC)),
    ))
}

fn with_bob_until_midnight() -> Roster {
    lichess_with_first(shift(
        user("PBOB001", "Bob Example"),
        Some(datetime!(2026-11-15 00:00 UTC)),
    ))
}

async fn answers_in_turn(server: &MockServer, at: &str, answers: &[ResponseTemplate]) {
    for answer in answers {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(answer.clone())
            .up_to_n_times(1)
            .mount(server)
            .await;
    }
    if let Some(last) = answers.last() {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(last.clone())
            .mount(server)
            .await;
    }
}

struct Syncing {
    pagerduty: MockServer,
    task: JoinHandle<()>,
    roster: watch::Receiver<Option<Roster>>,
    refresh: Arc<Notify>,
}

impl Syncing {
    async fn start(
        policies: &[ResponseTemplate],
        oncalls: &[ResponseTemplate],
        interval: Duration,
    ) -> Self {
        let pagerduty = MockServer::start().await;
        answers_in_turn(&pagerduty, ESCALATION_POLICIES, policies).await;
        answers_in_turn(&pagerduty, ONCALLS, oncalls).await;
        let api = pagerduty::Api::new(
            reqwest::Client::new(),
            &pagerduty::Config {
                token: SecretString::from("u+secret"),
                base_url: pagerduty.uri().parse().unwrap(),
                events: None,
            },
        );
        let (published, roster) = watch::channel(None);
        let refresh = Arc::new(Notify::new());
        let task = tokio::spawn(keep_in_sync(api, published, Arc::clone(&refresh), interval));
        Self {
            pagerduty,
            task,
            roster,
            refresh,
        }
    }

    async fn next_roster(&mut self) -> Roster {
        tokio::time::timeout(PATIENCE, self.roster.changed())
            .await
            .unwrap()
            .unwrap();
        self.roster.borrow_and_update().clone().unwrap()
    }

    async fn reads_reach(&self, at: &str, count: usize) {
        let reached = async {
            while self.reads_of(at).await < count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        tokio::time::timeout(PATIENCE, reached).await.unwrap();
    }

    async fn reads_of(&self, at: &str) -> usize {
        let requests = self.pagerduty.received_requests().await.unwrap();
        requests
            .iter()
            .filter(|request| request.url.path() == at)
            .count()
    }
}

impl Drop for Syncing {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn the_roster_read_from_pagerduty_is_published() {
    let mut syncing = Syncing::start(
        &[lichess_and_an_unused_policy()],
        &[alice_until_the_evening()],
        RARELY,
    )
    .await;

    assert_eq!(syncing.next_roster().await, with_alice_until_the_evening());
}

#[tokio::test]
async fn a_roster_that_changed_is_published_at_a_later_refresh_even_when_only_a_shift_end_moved() {
    let mut syncing = Syncing::start(
        &[lichess_and_an_unused_policy()],
        &[
            alice_until_the_evening(),
            alice_until_midnight(),
            bob_until_midnight(),
        ],
        OFTEN,
    )
    .await;

    assert_eq!(syncing.next_roster().await, with_alice_until_the_evening());
    assert_eq!(syncing.next_roster().await, with_alice_until_midnight());
    assert_eq!(syncing.next_roster().await, with_bob_until_midnight());
}

#[tokio::test]
async fn the_same_roster_is_not_published_again() {
    let mut syncing = Syncing::start(
        &[lichess_and_an_unused_policy()],
        &[alice_until_the_evening()],
        OFTEN,
    )
    .await;

    syncing.next_roster().await;
    syncing.reads_reach(ONCALLS, 4).await;

    assert!(!syncing.roster.has_changed().unwrap());
}

#[tokio::test]
async fn the_previous_roster_is_kept_when_who_is_on_call_cannot_be_read() {
    let mut syncing = Syncing::start(
        &[lichess_and_an_unused_policy()],
        &[alice_until_the_evening(), ResponseTemplate::new(502)],
        OFTEN,
    )
    .await;

    syncing.next_roster().await;
    syncing.reads_reach(ONCALLS, 4).await;

    assert!(!syncing.roster.has_changed().unwrap());
    assert_eq!(
        *syncing.roster.borrow(),
        Some(with_alice_until_the_evening())
    );
    assert!(!syncing.task.is_finished());
}

#[tokio::test]
async fn the_previous_roster_is_kept_when_the_escalation_policies_cannot_be_read() {
    let refusal = ResponseTemplate::new(401)
        .set_body_json(json!({ "error": { "message": "Unauthorized", "code": 2006 } }));
    let mut syncing = Syncing::start(
        &[lichess_and_an_unused_policy(), refusal],
        &[alice_until_the_evening(), bob_until_midnight()],
        OFTEN,
    )
    .await;

    syncing.next_roster().await;
    syncing.reads_reach(ESCALATION_POLICIES, 4).await;

    assert!(!syncing.roster.has_changed().unwrap());
    assert_eq!(
        *syncing.roster.borrow(),
        Some(with_alice_until_the_evening())
    );
    assert!(!syncing.task.is_finished());
}

#[tokio::test]
async fn a_roster_is_published_once_a_read_that_failed_at_first_works() {
    let mut syncing = Syncing::start(
        &[ResponseTemplate::new(502), lichess_and_an_unused_policy()],
        &[alice_until_the_evening()],
        OFTEN,
    )
    .await;

    assert_eq!(syncing.next_roster().await, with_alice_until_the_evening());
}

#[tokio::test]
async fn pagerduty_is_read_again_at_once_when_a_refresh_is_asked_for() {
    let mut syncing = Syncing::start(
        &[lichess_and_an_unused_policy()],
        &[alice_until_the_evening(), bob_until_midnight()],
        RARELY,
    )
    .await;
    assert_eq!(syncing.next_roster().await, with_alice_until_the_evening());

    syncing.refresh.notify_one();

    assert_eq!(syncing.next_roster().await, with_bob_until_midnight());
    assert_eq!(syncing.reads_of(ONCALLS).await, 2);
}
