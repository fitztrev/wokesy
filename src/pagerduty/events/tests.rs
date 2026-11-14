use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

use super::*;

async fn pagerduty_answering(request: Mock) -> (MockServer, Events) {
    let server = MockServer::start().await;
    request.mount(&server).await;
    let config = EventsConfig {
        routing_key: SecretString::from("samplekeyhere"),
        url: server.uri().parse().unwrap(),
    };
    (server, Events::new(reqwest::Client::new(), &config))
}

fn enqueue() -> MockBuilder {
    Mock::given(method("POST")).and(path("/v2/enqueue"))
}

async fn page(events: &Events) -> Result<(), ApiError> {
    let summary = Summary::try_new("Alice Example: lichess.org is down").unwrap();
    let paged_by = Name::try_new("Alice Example").unwrap();
    events.trigger(&summary, &paged_by).await
}

#[tokio::test]
async fn a_page_is_enqueued_with_the_routing_key_in_its_body_and_no_authorization() {
    let request = enqueue()
        .and(header(CONTENT_TYPE, "application/json"))
        .and(body_json(json!({
            "routing_key": "samplekeyhere",
            "event_action": "trigger",
            "payload": {
                "summary": "Alice Example: lichess.org is down",
                "source": "Zulip",
                "severity": "critical",
                "custom_details": { "paged_by": "Alice Example" },
            },
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "status": "success",
            "message": "Event processed",
            "dedup_key": "srv01/HTTP",
        })))
        .expect(1);
    let (server, events) = pagerduty_answering(request).await;

    page(&events).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| !request.headers.contains_key(AUTHORIZATION))
    );
}

#[tokio::test]
async fn a_refused_event_is_a_rejection_carrying_its_message_and_its_errors() {
    let answer = enqueue().respond_with(ResponseTemplate::new(400).set_body_json(json!({
        "status": "invalid event",
        "message": "Event object is invalid",
        "errors": ["Length of 'routing_key' is incorrect (should be 32 characters)"],
    })));
    let (_server, events) = pagerduty_answering(answer).await;

    assert!(matches!(
        page(&events).await,
        Err(ApiError::Rejected { status: StatusCode::BAD_REQUEST, reason })
            if reason == "Event object is invalid: Length of 'routing_key' is incorrect (should be 32 characters)"
    ));
}

#[tokio::test]
async fn a_failing_answer_of_another_shape_is_a_rejection_carrying_the_reason_phrase() {
    let answer = enqueue().respond_with(ResponseTemplate::new(429));
    let (_server, events) = pagerduty_answering(answer).await;

    assert!(matches!(
        page(&events).await,
        Err(ApiError::Rejected { status: StatusCode::TOO_MANY_REQUESTS, reason })
            if reason == "Too Many Requests"
    ));
}
