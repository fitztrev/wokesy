use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use super::*;
use crate::health::{Recovery, Unavailable};

fn unavailable() -> Availability {
    Availability::Unavailable(Unavailable {
        recovery: Recovery::Recoverable,
        error: "pagerduty request failed".to_owned(),
    })
}

fn pagerduty_unavailable() -> Health {
    Health {
        pagerduty: unavailable(),
        zulip: Availability::Available,
    }
}

async fn status(health: Option<Health>, path: &str) -> StatusCode {
    let request = Request::get(path).body(Body::empty()).unwrap();
    router(watch::channel(health).1)
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn livez_answers_200_before_health_has_checked() {
    assert_eq!(status(None, "/livez").await, StatusCode::OK);
}

#[tokio::test]
async fn livez_answers_200_while_pagerduty_is_unavailable() {
    assert_eq!(
        status(Some(pagerduty_unavailable()), "/livez").await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn readyz_answers_503_before_health_has_checked() {
    assert_eq!(
        status(None, "/readyz").await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn readyz_answers_200_while_pagerduty_is_available_even_when_zulip_is_not() {
    let health = Health {
        pagerduty: Availability::Available,
        zulip: unavailable(),
    };

    assert_eq!(status(Some(health), "/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_answers_503_while_pagerduty_is_unavailable() {
    assert_eq!(
        status(Some(pagerduty_unavailable()), "/readyz").await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
