use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use secrecy::SecretString;
use serde_json::{Value, json};
use time::macros::datetime;
use tokio::net::TcpListener;
use tokio::time::Instant;
use wiremock::matchers::{body_json, header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

use super::*;
use crate::pagerduty::{
    EscalationLevel, EscalationPolicy, EscalationPolicyId, EscalationPolicyName, EscalationRule,
    OnCall, OverrideId, ScheduleId, ScheduleName, ScheduleRef, Target, User, UserId, UserRef,
};
use crate::people::{Email, Name};
use crate::testing::Logs;

fn api_at(base_url: &str) -> Api {
    let config = Config {
        token: SecretString::from("u+secret"),
        base_url: base_url.parse().unwrap(),
        events: None,
    };
    Api::new(reqwest::Client::new(), &config)
}

async fn pagerduty() -> (MockServer, Api) {
    let server = MockServer::start().await;
    let api = api_at(&server.uri());
    (server, api)
}

async fn pagerduty_answering(request: Mock) -> (MockServer, Api) {
    let (server, api) = pagerduty().await;
    request.mount(&server).await;
    (server, api)
}

async fn a_server_that_never_answers() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((connection, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _kept_open = connection;
                std::future::pending::<()>().await;
            });
        }
    });
    format!("http://{address}")
}

fn gave_up_waiting<T: std::fmt::Debug>(answer: &Result<T, ApiError>) -> bool {
    matches!(answer, Err(ApiError::Request(error)) if error.is_timeout())
}

fn get(asked: &str) -> MockBuilder {
    Mock::given(method("GET")).and(path(asked))
}

fn user_id(id: &str) -> UserId {
    UserId::try_new(id).unwrap()
}

fn schedule_id(id: &str) -> ScheduleId {
    ScheduleId::try_new(id).unwrap()
}

fn override_id(id: &str) -> OverrideId {
    OverrideId::try_new(id).unwrap()
}

fn user(id: &str, name: &str) -> UserRef {
    UserRef {
        id: user_id(id),
        name: Name::try_new(name).unwrap(),
    }
}

fn schedule(id: &str, name: &str) -> ScheduleRef {
    ScheduleRef {
        id: schedule_id(id),
        name: ScheduleName::try_new(name).unwrap(),
    }
}

fn users_page(offset: usize, ids: &[&str], more: bool) -> Mock {
    let users: Vec<Value> = ids
        .iter()
        .map(|id| json!({ "id": id, "name": "Earline Greenholt", "email": "e@example.com" }))
        .collect();
    get("/users")
        .and(query_param("limit", "100"))
        .and(query_param("offset", offset.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "users": users,
            "limit": 100,
            "offset": offset,
            "more": more,
        })))
        .expect(1)
}

#[tokio::test]
async fn every_request_carries_the_token_and_asks_for_version_2() {
    let request = Mock::given(header(AUTHORIZATION, "Token token=u+secret"))
        .and(header(ACCEPT, "application/vnd.pagerduty+json;version=2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "users": [],
            "escalation_policies": [],
            "oncalls": [],
            "override": { "id": "PQ47DCP" },
            "more": false,
        })))
        .expect(6);
    let (_server, api) = pagerduty_answering(request).await;
    let rota = schedule_id("PI7DH85");
    let start = datetime!(2026-11-14 20:00 UTC);
    let end = datetime!(2026-11-15 00:00 UTC);

    api.abilities().await.unwrap();
    api.users().await.unwrap();
    api.escalation_policies().await.unwrap();
    api.oncalls().await.unwrap();
    api.create_schedule_override(&rota, &user_id("PEYSGVF"), start, end)
        .await
        .unwrap();
    api.delete_schedule_override(&rota, &override_id("PQ47DCP"))
        .await
        .unwrap();
}

#[tokio::test]
async fn paths_are_joined_onto_a_base_url_with_a_prefix_with_or_without_a_trailing_slash() {
    let server = MockServer::start().await;
    get("/proxy/pagerduty/abilities")
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&server)
        .await;

    for prefix in ["/proxy/pagerduty", "/proxy/pagerduty/"] {
        let api = api_at(&format!("{}{prefix}", server.uri()));
        api.abilities().await.unwrap();
    }
}

#[tokio::test]
async fn a_list_spread_over_several_pages_is_read_to_the_end() {
    let (server, api) = pagerduty().await;
    users_page(0, &["P000001", "P000002"], true)
        .mount(&server)
        .await;
    users_page(100, &["P000003"], true).mount(&server).await;
    users_page(200, &["P000004"], false).mount(&server).await;

    let users = api.users().await.unwrap();

    let ids: Vec<&str> = users.iter().map(|user| user.id.as_str()).collect();
    assert_eq!(ids, ["P000001", "P000002", "P000003", "P000004"]);
}

#[tokio::test]
async fn a_user_is_read_with_its_name_and_email() {
    let answer = get("/users").respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "users": [{
            "id": "PXPGF42",
            "name": "Earline Greenholt",
            "email": "E@Example.com",
            "time_zone": "America/Lima",
            "role": "admin",
        }],
        "more": false,
    })));
    let (_server, api) = pagerduty_answering(answer).await;

    assert_eq!(
        api.users().await.unwrap(),
        [User {
            id: user_id("PXPGF42"),
            name: Name::try_new("Earline Greenholt").unwrap(),
            email: Email::try_new("e@example.com").unwrap(),
        }]
    );
}

#[tokio::test]
async fn the_rules_of_an_escalation_policy_become_targets_in_order_and_services_put_it_in_use() {
    let answer = get("/escalation_policies").respond_with(ResponseTemplate::new(200).set_body_json(
        json!({
            "escalation_policies": [
                {
                    "id": "PANZZEQ",
                    "name": "Engineering",
                    "escalation_rules": [
                        {
                            "id": "PGHDV41",
                            "escalation_delay_in_minutes": 30,
                            "targets": [
                                { "id": "PEYSGVF", "type": "user_reference", "summary": "PagerDuty Admin" },
                                { "id": "PI7DH85", "type": "schedule_reference", "summary": "Daily Rotation" },
                            ],
                        },
                        {
                            "id": "PGHDV42",
                            "escalation_delay_in_minutes": 30,
                            "targets": [
                                { "id": "PI7DH86", "type": "schedule", "summary": "Weekly Rotation" },
                                { "id": "PXPGF42", "type": "user", "summary": "Earline Greenholt" },
                            ],
                        },
                    ],
                    "services": [{ "id": "PIJ90N7", "type": "service_reference", "summary": "Mail" }],
                    "num_loops": 2,
                },
                {
                    "id": "PT20YPA",
                    "name": "Unused",
                    "escalation_rules": [],
                    "services": [],
                },
            ],
            "more": false,
        }),
    ));
    let (_server, api) = pagerduty_answering(answer).await;

    assert_eq!(
        api.escalation_policies().await.unwrap(),
        [
            EscalationPolicy {
                id: EscalationPolicyId::try_new("PANZZEQ").unwrap(),
                name: EscalationPolicyName::try_new("Engineering").unwrap(),
                rules: vec![
                    EscalationRule {
                        targets: vec![
                            Target::User(user("PEYSGVF", "PagerDuty Admin")),
                            Target::Schedule(schedule("PI7DH85", "Daily Rotation")),
                        ],
                    },
                    EscalationRule {
                        targets: vec![
                            Target::Schedule(schedule("PI7DH86", "Weekly Rotation")),
                            Target::User(user("PXPGF42", "Earline Greenholt")),
                        ],
                    },
                ],
                in_use: true,
            },
            EscalationPolicy {
                id: EscalationPolicyId::try_new("PT20YPA").unwrap(),
                name: EscalationPolicyName::try_new("Unused").unwrap(),
                rules: vec![],
                in_use: false,
            },
        ]
    );
}

#[tokio::test]
async fn an_on_call_entry_without_a_schedule_or_an_end_is_a_directly_targeted_person() {
    let request = get("/oncalls")
        .and(query_param("earliest", "true"))
        .and(query_param_is_missing("since"))
        .and(query_param_is_missing("until"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "oncalls": [
                {
                    "escalation_policy": { "id": "PT20YPA", "type": "escalation_policy_reference", "summary": "Engineering" },
                    "escalation_level": 2,
                    "schedule": { "id": "PI7DH85", "type": "schedule_reference", "summary": "Daily Rotation" },
                    "user": { "id": "PXPGF42", "type": "user_reference", "summary": "Earline Greenholt" },
                    "start": "2015-03-06T15:28:51-05:00",
                    "end": "2015-03-07T15:28:51-05:00",
                },
                {
                    "escalation_policy": { "id": "PT20YPA", "type": "escalation_policy_reference", "summary": "Engineering" },
                    "escalation_level": 1,
                    "schedule": null,
                    "user": { "id": "PEYSGVF", "type": "user_reference", "summary": "PagerDuty Admin" },
                    "start": null,
                    "end": null,
                },
            ],
            "more": false,
        })));
    let (_server, api) = pagerduty_answering(request).await;
    let engineering = EscalationPolicyId::try_new("PT20YPA").unwrap();

    assert_eq!(
        api.oncalls().await.unwrap(),
        [
            OnCall {
                escalation_policy: engineering.clone(),
                level: EscalationLevel::new(2),
                schedule: Some(schedule_id("PI7DH85")),
                user: user("PXPGF42", "Earline Greenholt"),
                end: Some(datetime!(2015-03-07 15:28:51 -5)),
            },
            OnCall {
                escalation_policy: engineering,
                level: EscalationLevel::new(1),
                schedule: None,
                user: user("PEYSGVF", "PagerDuty Admin"),
                end: None,
            },
        ]
    );
}

#[tokio::test]
async fn a_user_whose_name_or_email_cannot_be_used_is_skipped_and_the_others_are_read() {
    let (logs, _listening) = Logs::listen();
    let answer = get("/users").respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "users": [
            { "id": "PGOOD01", "name": "Earline Greenholt", "email": "e@example.com" },
            { "id": "PBAD001", "name": "  ", "email": "nameless@example.com" },
            { "id": "PBAD002", "name": "Mallory Example", "email": "not-an-address" },
            { "id": "PBAD003", "name": "Nobody Example", "email": null },
            { "id": "PGOOD02", "name": "Aurelio Rice", "email": "a@example.com" },
        ],
        "more": false,
    })));
    let (_server, api) = pagerduty_answering(answer).await;

    let users = api.users().await.unwrap();

    let ids: Vec<&str> = users.iter().map(|user| user.id.as_str()).collect();
    let logged = logs.text();
    assert_eq!(ids, ["PGOOD01", "PGOOD02"]);
    for skipped in ["PBAD001", "PBAD002", "PBAD003"] {
        assert!(
            logged.contains(&format!(
                "user that cannot be read skipped user=\"{skipped}\""
            )),
            "{logged}"
        );
    }
    for secret in ["Mallory", "Nobody", "not-an-address", "example.com"] {
        assert!(!logged.contains(secret), "{secret} is in {logged}");
    }
}

#[tokio::test]
async fn an_on_call_entry_without_a_user_is_skipped_and_the_others_are_read() {
    let (logs, _listening) = Logs::listen();
    let engineering =
        json!({ "id": "PT20YPA", "type": "escalation_policy_reference", "summary": "Engineering" });
    let request = get("/oncalls").respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "oncalls": [
            {
                "escalation_policy": engineering,
                "escalation_level": 1,
                "schedule": { "id": "PI7DH85", "type": "schedule_reference", "summary": "Daily Rotation" },
                "user": null,
                "start": null,
                "end": null,
            },
            {
                "escalation_policy": engineering,
                "escalation_level": 2,
                "schedule": null,
                "user": { "id": "PEYSGVF", "type": "user_reference", "summary": "PagerDuty Admin" },
                "start": null,
                "end": null,
            },
        ],
        "more": false,
    })));
    let (_server, api) = pagerduty_answering(request).await;

    let oncalls = api.oncalls().await.unwrap();

    let logged = logs.text();
    assert_eq!(
        oncalls,
        [OnCall {
            escalation_policy: EscalationPolicyId::try_new("PT20YPA").unwrap(),
            level: EscalationLevel::new(2),
            schedule: None,
            user: user("PEYSGVF", "PagerDuty Admin"),
            end: None,
        }]
    );
    assert!(
        logged.contains(
            "on-call entry that cannot be read skipped escalation_policy=\"PT20YPA\" schedule=\"PI7DH85\" user=\"unknown\""
        ),
        "{logged}"
    );
    assert!(
        !logged.contains("Engineering") && !logged.contains("Daily Rotation"),
        "{logged}"
    );
}

#[tokio::test]
async fn a_target_of_an_unknown_type_is_skipped_and_the_rest_of_its_policy_is_read() {
    let (logs, _listening) = Logs::listen();
    let answer = get("/escalation_policies").respond_with(ResponseTemplate::new(200).set_body_json(
        json!({
            "escalation_policies": [
                {
                    "id": "PANZZEQ",
                    "name": "Engineering",
                    "escalation_rules": [{
                        "targets": [
                            { "id": "PTEAM01", "type": "team_reference", "summary": "Platform team" },
                            { "id": "PI7DH85", "type": "schedule_reference", "summary": "Daily Rotation" },
                        ],
                    }],
                    "services": [{ "id": "PIJ90N7", "type": "service_reference", "summary": "Mail" }],
                },
                { "id": "PNONAME", "name": "", "escalation_rules": [], "services": [] },
                { "id": "PT20YPA", "name": "Unused", "escalation_rules": [], "services": [] },
            ],
            "more": false,
        }),
    ));
    let (_server, api) = pagerduty_answering(answer).await;

    let policies = api.escalation_policies().await.unwrap();

    let logged = logs.text();
    assert_eq!(
        policies,
        [
            EscalationPolicy {
                id: EscalationPolicyId::try_new("PANZZEQ").unwrap(),
                name: EscalationPolicyName::try_new("Engineering").unwrap(),
                rules: vec![EscalationRule {
                    targets: vec![Target::Schedule(schedule("PI7DH85", "Daily Rotation"))],
                }],
                in_use: true,
            },
            EscalationPolicy {
                id: EscalationPolicyId::try_new("PT20YPA").unwrap(),
                name: EscalationPolicyName::try_new("Unused").unwrap(),
                rules: vec![],
                in_use: false,
            },
        ]
    );
    assert!(
        logged.contains(
            "escalation target that cannot be read skipped escalation_policy=PANZZEQ target=\"PTEAM01\""
        ),
        "{logged}"
    );
    assert!(
        logged.contains(
            "escalation policy that cannot be read skipped escalation_policy=\"PNONAME\""
        ),
        "{logged}"
    );
    assert!(!logged.contains("Platform team"), "{logged}");
}

#[tokio::test]
async fn a_list_that_is_not_a_list_of_records_is_unreadable() {
    for users in [json!({ "id": "PXPGF42" }), json!("PXPGF42"), json!(null)] {
        let answer = get("/users").respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "users": users, "more": false })),
        );
        let (_server, api) = pagerduty_answering(answer).await;

        assert!(
            matches!(
                api.users().await,
                Err(ApiError::Unreadable {
                    status: StatusCode::OK,
                    ..
                })
            ),
            "{users}"
        );
    }
}

fn create_override() -> MockBuilder {
    Mock::given(method("POST")).and(path("/schedules/PI7DH85/overrides"))
}

async fn created_override(api: &Api) -> Result<OverrideId, ApiError> {
    api.create_schedule_override(
        &schedule_id("PI7DH85"),
        &user_id("PEYSGVF"),
        datetime!(2026-11-14 21:00:00.25 +1),
        datetime!(2026-11-15 01:00:00.25 +1),
    )
    .await
}

#[tokio::test]
async fn an_override_is_created_for_a_user_between_two_times_in_utc() {
    let request = create_override()
        .and(header(CONTENT_TYPE, "application/json"))
        .and(body_json(json!({
            "override": {
                "start": "2026-11-14T20:00:00Z",
                "end": "2026-11-15T00:00:00Z",
                "user": { "id": "PEYSGVF", "type": "user_reference" },
            },
        })))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({ "override": { "id": "PQ47DCP" } })),
        )
        .expect(1);
    let (_server, api) = pagerduty_answering(request).await;

    created_override(&api).await.unwrap();
}

#[tokio::test]
async fn the_id_of_a_created_override_is_read_from_either_shape_of_the_answer() {
    for answer in [
        json!({
            "override": {
                "id": "PQ47DCP",
                "start": "2026-11-14T20:00:00Z",
                "end": "2026-11-15T00:00:00Z",
                "user": { "id": "PEYSGVF", "type": "user_reference", "summary": "Aurelio Rice" },
            },
        }),
        json!([
            {
                "status": 201,
                "override": {
                    "id": "PQ47DCP",
                    "start": "2026-11-14T20:00:00Z",
                    "end": "2026-11-15T00:00:00Z",
                    "user": { "id": "PEYSGVF", "type": "user_reference", "summary": "Aurelio Rice" },
                },
            },
            { "status": 201, "override": { "id": "PZ99ZZZ" } },
        ]),
    ] {
        let request =
            create_override().respond_with(ResponseTemplate::new(201).set_body_json(&answer));
        let (_server, api) = pagerduty_answering(request).await;

        assert_eq!(
            created_override(&api).await.unwrap(),
            override_id("PQ47DCP"),
            "{answer}"
        );
    }
}

#[tokio::test]
async fn a_created_override_whose_answer_carries_no_id_is_unreadable() {
    for answer in [
        json!({ "overrides": [{ "id": "PQ47DCP" }] }),
        json!([{ "status": 400, "errors": ["Override must end after its start"] }]),
        json!([]),
    ] {
        let request =
            create_override().respond_with(ResponseTemplate::new(201).set_body_json(&answer));
        let (_server, api) = pagerduty_answering(request).await;

        assert!(
            matches!(
                created_override(&api).await,
                Err(ApiError::Unreadable {
                    status: StatusCode::CREATED,
                    ..
                })
            ),
            "{answer}"
        );
    }
}

#[tokio::test]
async fn an_override_is_deleted_by_its_id_and_both_204_and_200_are_success() {
    for status in [204, 200] {
        let request = Mock::given(method("DELETE"))
            .and(path("/schedules/PI7DH85/overrides/PQ47DCP"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1);
        let (_server, api) = pagerduty_answering(request).await;

        api.delete_schedule_override(&schedule_id("PI7DH85"), &override_id("PQ47DCP"))
            .await
            .unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn an_override_that_pagerduty_does_not_answer_is_given_up_after_ten_seconds() {
    let api = api_at(&a_server_that_never_answers().await);
    let ten_seconds = Duration::from_secs(10)..Duration::from_secs(11);

    let asked = Instant::now();
    let created = created_override(&api).await;
    let waited_to_create = asked.elapsed();
    let asked = Instant::now();
    let deleted = api
        .delete_schedule_override(&schedule_id("PI7DH85"), &override_id("PQ47DCP"))
        .await;
    let waited_to_delete = asked.elapsed();

    assert!(gave_up_waiting(&created), "{created:?}");
    assert!(gave_up_waiting(&deleted), "{deleted:?}");
    assert!(
        ten_seconds.contains(&waited_to_create),
        "{waited_to_create:?}"
    );
    assert!(
        ten_seconds.contains(&waited_to_delete),
        "{waited_to_delete:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_read_that_pagerduty_does_not_answer_is_given_up_after_thirty_seconds() {
    let api = api_at(&a_server_that_never_answers().await);

    let asked = Instant::now();
    let users = api.users().await;
    let waited = asked.elapsed();

    assert!(gave_up_waiting(&users), "{users:?}");
    assert!(
        (Duration::from_secs(30)..Duration::from_secs(31)).contains(&waited),
        "{waited:?}"
    );
}

#[tokio::test]
async fn an_error_answer_is_a_rejection_carrying_its_message_and_its_errors() {
    let answer = get("/users").respond_with(ResponseTemplate::new(400).set_body_json(json!({
        "error": {
            "message": "Invalid Input Provided",
            "code": 2001,
            "errors": ["Offset must be a positive integer", "Limit is too large"],
        },
    })));
    let (_server, api) = pagerduty_answering(answer).await;

    let error = api.users().await.unwrap_err();

    assert!(matches!(
        &error,
        ApiError::Rejected { status: StatusCode::BAD_REQUEST, reason }
            if reason == "Invalid Input Provided: Offset must be a positive integer; Limit is too large"
    ));
    assert_eq!(
        error.to_string(),
        "pagerduty rejected the request (400 Bad Request): Invalid Input Provided: Offset must be a positive integer; Limit is too large"
    );
}

#[tokio::test]
async fn an_error_answer_without_errors_is_a_rejection_carrying_its_message() {
    let answer = get("/abilities").respond_with(
        ResponseTemplate::new(401)
            .set_body_json(json!({ "error": { "message": "Unauthorized", "code": 2006 } })),
    );
    let (_server, api) = pagerduty_answering(answer).await;

    assert!(matches!(
        api.abilities().await,
        Err(ApiError::Rejected { status: StatusCode::UNAUTHORIZED, reason }) if reason == "Unauthorized"
    ));
}

#[tokio::test]
async fn a_failing_answer_of_another_shape_is_a_rejection_carrying_the_reason_phrase() {
    let answer = get("/oncalls")
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"));
    let (_server, api) = pagerduty_answering(answer).await;

    assert!(matches!(
        api.oncalls().await,
        Err(ApiError::Rejected { status: StatusCode::BAD_GATEWAY, reason }) if reason == "Bad Gateway"
    ));
}

#[tokio::test]
async fn a_successful_answer_of_an_unexpected_shape_is_unreadable() {
    let answer = get("/users").respond_with(
        ResponseTemplate::new(200).set_body_json(json!({ "people": [], "more": false })),
    );
    let (_server, api) = pagerduty_answering(answer).await;

    assert!(matches!(
        api.users().await,
        Err(ApiError::Unreadable {
            status: StatusCode::OK,
            ..
        })
    ));
}
