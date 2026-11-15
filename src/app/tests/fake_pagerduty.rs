use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::State;
use axum::http::header::USER_AGENT;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::watch;
use url::Url;

pub const ALICE: &str = "PALICE1";
pub const BOB: &str = "PBOB001";
pub const CAROL: &str = "PCAROL1";
pub const PRIMARY_ROTA: &str = "PI7DH85";
pub const PRIMARY_ROTA_OVERRIDES: &str = "/schedules/PI7DH85/overrides";
pub const PRIMARY_ROTA_SHIFTS: &str = "/v3/schedules/PI7DH85/custom_shifts";
pub const PRIMARY_ROTA_SHIFT: &str = "/v3/schedules/PI7DH85/custom_shifts/PCS47DC";
pub const ENQUEUE: &str = "/v2/enqueue";
pub const ON_CALL: &str = "/oncalls";

const LICHESS: &str = "PLICHES";
const BOB_IS_ON_CALL_UNTIL: &str = "2026-11-14T20:00:00Z";
const SHIFT_BASED: &str = "This is a shift-based schedule. Use the v3 Schedules API to access it.";
const PATIENCE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub method: Method,
    pub path: String,
    pub user_agent: String,
    pub body: Value,
}

impl Asked {
    pub fn is(&self, method: &Method, path: &str) -> bool {
        self.method == method && self.path == path
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rota {
    LayerBased,
    ShiftBased,
}

#[derive(Clone)]
struct Account {
    rota: Rota,
    asked: watch::Sender<Vec<Asked>>,
    overriding: watch::Sender<Option<Value>>,
    custom_shift: watch::Sender<Option<Value>>,
    pages_held_back: watch::Sender<bool>,
}

pub struct FakePagerDuty {
    pub url: Url,
    account: Account,
}

impl FakePagerDuty {
    pub async fn start() -> Self {
        Self::start_with(Rota::LayerBased).await
    }

    pub async fn start_with_a_shift_based_rota() -> Self {
        Self::start_with(Rota::ShiftBased).await
    }

    async fn start_with(rota: Rota) -> Self {
        let account = Account {
            rota,
            asked: watch::Sender::new(Vec::new()),
            overriding: watch::Sender::new(None),
            custom_shift: watch::Sender::new(None),
            pages_held_back: watch::Sender::new(false),
        };
        let router = Router::new().fallback(answer).with_state(account.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            url: format!("http://{address}").parse().unwrap(),
            account,
        }
    }

    pub fn asked(&self) -> Vec<Asked> {
        self.account.asked.borrow().clone()
    }

    pub fn changes_asked(&self) -> Vec<Asked> {
        self.asked()
            .into_iter()
            .filter(|asked| asked.method != Method::GET)
            .collect()
    }

    pub async fn was_asked(&self, method: Method, path: &str) -> Asked {
        let mut asked = self.account.asked.subscribe();
        let all = tokio::time::timeout(
            PATIENCE,
            asked.wait_for(|all| all.iter().any(|asked| asked.is(&method, path))),
        )
        .await
        .unwrap()
        .unwrap();
        all.iter()
            .find(|asked| asked.is(&method, path))
            .unwrap()
            .clone()
    }

    pub fn holds_pages_back(&self) {
        self.account.pages_held_back.send_replace(true);
    }

    pub fn lets_pages_through(&self) {
        self.account.pages_held_back.send_replace(false);
    }
}

async fn answer(
    State(account): State<Account>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: String,
) -> Response {
    let asked = Asked {
        method,
        path: uri.path().to_owned(),
        user_agent: String::from_utf8_lossy(headers[USER_AGENT].as_bytes()).into_owned(),
        body: serde_json::from_str(&body).unwrap_or(Value::Null),
    };
    account.asked.send_modify(|all| all.push(asked.clone()));
    let shift_based = account.rota == Rota::ShiftBased;
    let (status, answer) = match (asked.method.as_str(), asked.path.as_str()) {
        ("GET", "/abilities") => (StatusCode::OK, json!({ "abilities": ["teams"] })),
        ("GET", "/users") => (StatusCode::OK, users()),
        ("GET", "/escalation_policies") => (StatusCode::OK, escalation_policies(account.rota)),
        ("GET", ON_CALL) => (
            StatusCode::OK,
            on_call(
                account.rota,
                account.overriding.borrow().as_ref(),
                account.custom_shift.borrow().as_ref(),
            ),
        ),
        ("POST", PRIMARY_ROTA_OVERRIDES) if shift_based => (
            StatusCode::BAD_REQUEST,
            json!({ "error": { "message": SHIFT_BASED, "code": 3005, "errors": [SHIFT_BASED] } }),
        ),
        ("POST", PRIMARY_ROTA_SHIFTS) if shift_based => {
            let created = asked.body["custom_shifts"][0].clone();
            account.custom_shift.send_replace(Some(created.clone()));
            (
                StatusCode::CREATED,
                json!({ "custom_shifts": [with(created, "id", json!("PCS47DC"))] }),
            )
        }
        ("DELETE", PRIMARY_ROTA_SHIFT) if account.custom_shift.borrow().is_some() => {
            account.custom_shift.send_replace(None);
            return StatusCode::NO_CONTENT.into_response();
        }
        ("POST", PRIMARY_ROTA_OVERRIDES) => {
            let created = asked.body["override"].clone();
            account.overriding.send_replace(Some(created.clone()));
            (
                StatusCode::CREATED,
                json!({ "override": with(created, "id", json!("PQ47DCP")) }),
            )
        }
        ("POST", ENQUEUE) => {
            let mut held_back = account.pages_held_back.subscribe();
            held_back.wait_for(|held_back| !held_back).await.unwrap();
            (
                StatusCode::ACCEPTED,
                json!({ "status": "success", "message": "Event processed", "dedup_key": "srv01/HTTP" }),
            )
        }
        _ => (
            StatusCode::NOT_FOUND,
            json!({ "error": { "message": "Not Found", "code": 2100 } }),
        ),
    };
    (status, Json(answer)).into_response()
}

fn with(mut object: Value, field: &str, value: Value) -> Value {
    object[field] = value;
    object
}

fn page(of: &str, entries: &Value) -> Value {
    json!({ of: entries, "limit": 100, "offset": 0, "more": false, "total": null })
}

fn name_of(user: &str) -> &'static str {
    match user {
        ALICE => "Alice Example",
        BOB => "Bob Example",
        _ => "Carol Example",
    }
}

fn user_reference(id: &str) -> Value {
    json!({ "id": id, "type": "user_reference", "summary": name_of(id) })
}

fn primary_rota_reference(rota: Rota) -> Value {
    let reference = match rota {
        Rota::LayerBased => "schedule_reference",
        Rota::ShiftBased => "schedule_v3_reference",
    };
    json!({ "id": PRIMARY_ROTA, "type": reference, "summary": "Primary rota" })
}

fn users() -> Value {
    let user = |id: &str, email: &str| json!({ "id": id, "type": "user", "name": name_of(id), "email": email, "time_zone": "Etc/UTC" });
    page(
        "users",
        &json!([
            user(ALICE, "alice@example.com"),
            user(BOB, "bob@pagerduty.example.com"),
            user(CAROL, "carol@example.com"),
        ]),
    )
}

fn escalation_policies(rota: Rota) -> Value {
    let lichess = json!({
        "id": LICHESS,
        "type": "escalation_policy",
        "name": "Lichess",
        "escalation_rules": [
            {
                "id": "PRULE01",
                "escalation_delay_in_minutes": 30,
                "targets": [primary_rota_reference(rota)],
            },
            {
                "id": "PRULE02",
                "escalation_delay_in_minutes": 30,
                "targets": [user_reference(CAROL)],
            },
        ],
        "services": [{ "id": "PIJ90N7", "type": "service_reference", "summary": "lichess.org" }],
        "num_loops": 0,
    });
    let unused = json!({
        "id": "PUNUSED",
        "type": "escalation_policy",
        "name": "Default",
        "escalation_rules": [
            { "id": "PRULE03", "escalation_delay_in_minutes": 30, "targets": [user_reference(ALICE)] },
        ],
        "services": [],
        "num_loops": 0,
    });
    page("escalation_policies", &json!([unused, lichess]))
}

fn on_call(rota: Rota, overriding: Option<&Value>, custom_shift: Option<&Value>) -> Value {
    let lichess =
        json!({ "id": LICHESS, "type": "escalation_policy_reference", "summary": "Lichess" });
    let (first, until) = overriding.map_or((BOB, BOB_IS_ON_CALL_UNTIL), |overriding| {
        (
            overriding["user"]["id"].as_str().unwrap(),
            overriding["end"].as_str().unwrap(),
        )
    });
    let on_the_rota = |user: &str, until: &str| {
        json!({
            "escalation_policy": lichess,
            "escalation_level": 1,
            "schedule": primary_rota_reference(rota),
            "user": user_reference(user),
            "start": "2026-11-14T08:00:00Z",
            "end": until,
        })
    };
    let added = custom_shift.map(|custom_shift| {
        on_the_rota(
            custom_shift["assignments"][0]["member"]["user_id"]
                .as_str()
                .unwrap(),
            custom_shift["end_time"].as_str().unwrap(),
        )
    });
    let further_up = json!({
        "escalation_policy": lichess,
        "escalation_level": 2,
        "schedule": null,
        "user": user_reference(CAROL),
        "start": null,
        "end": null,
    });
    let entries: Vec<Value> = std::iter::once(on_the_rota(first, until))
        .chain(added)
        .chain(std::iter::once(further_up))
        .collect();
    page("oncalls", &json!(entries))
}
