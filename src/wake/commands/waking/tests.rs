use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::on_call::Roster;
use crate::wake::Config;
use crate::wake::testing::{
    ALICE, Answering, BOB, FISHNET_OVERRIDES, PRIMARY_OVERRIDES, PRIMARY_SHIFTS, STRANGER, Setup,
    fishnet_rota, in_sysadmin, level, lichess_alone, policy, primary_rota, schedule, user,
    waking_on,
};

const BACKUP_OVERRIDES: &str = "/schedules/PBACKUP/overrides";
const SPARE_OVERRIDES: &str = "/schedules/PSPARE1/overrides";
const SPARE_SHIFTS: &str = "/v3/schedules/PSPARE1/custom_shifts";
const WAKING_IS_OFF: &str = "Waking is not set up here. Change the schedule in PagerDuty directly.";
const HANDED_BACK: &str = "PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.";
const SHIFT_ENDS: &str = "PagerDuty ends the shift by itself when the time is up. Send `stop waking me` to end it sooner.";
const FORBIDDEN: &str = "pagerduty rejected the request (403 Forbidden): Forbidden";

fn at(time: &Value) -> OffsetDateTime {
    OffsetDateTime::parse(time.as_str().unwrap(), &Rfc3339).unwrap()
}

fn whole_second(time: OffsetDateTime) -> OffsetDateTime {
    time.replace_nanosecond(0).unwrap()
}

async fn did_nothing(answering: &Answering) {
    assert_eq!(answering.requests().await, Vec::<String>::new());
    assert!(!answering.was_asked_to_refresh().await);
}

async fn answers_a_create(answering: &Answering, overrides: &str, answer: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(overrides))
        .respond_with(answer)
        .mount(&answering.pagerduty)
        .await;
}

fn woken_until(created: &Value, lines: &[&str]) -> String {
    format!(
        ":alarm_clock: I'll wake you for anything until <time:{}>.\n{}\n{HANDED_BACK}",
        created["override"]["end"].as_str().unwrap(),
        lines.join("\n")
    )
}

fn three_policies() -> Roster {
    let dave = user("PDAVE01", "Dave Example");
    let erin = user("PERIN01", "Erin Example");
    let frank = user("PFRANK1", "Frank Example");
    Roster {
        policies: vec![
            policy("PFISHNE", "Fishnet", &[level(1, &[fishnet_rota()], &[])]),
            policy(
                "PLICHES",
                "Lichess",
                &[
                    level(1, &[primary_rota()], &[dave, erin.clone()]),
                    level(2, &[schedule("PBACKUP", "Backup rota")], &[]),
                ],
            ),
            policy(
                "PTOURNA",
                "Tournaments",
                &[level(
                    1,
                    &[schedule("PTOURNA", "Tournament rota"), primary_rota()],
                    &[erin, frank],
                )],
            ),
        ],
    }
}

#[tokio::test]
async fn an_override_is_created_for_the_sender_from_now_for_as_long_as_asked() {
    let mut answering = Setup::default().start().await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    let before = whole_second(OffsetDateTime::now_utc());

    let reply = answering
        .reply(&in_sysadmin(BOB, "wake me for 90 minutes"))
        .await;

    let after = OffsetDateTime::now_utc();
    assert_eq!(
        answering.requests().await,
        [format!("POST {PRIMARY_OVERRIDES}")]
    );
    let created = answering.posted().await.remove(0);
    let shift = &created["override"];
    assert_eq!(
        shift["user"],
        json!({ "id": "PBOB001", "type": "user_reference" })
    );
    let (start, end) = (at(&shift["start"]), at(&shift["end"]));
    assert!(before <= start && start <= after, "{start}");
    assert_eq!(end - start, Duration::from_mins(90));
    assert_eq!(
        reply,
        woken_until(
            &created,
            &["- **Primary rota**: you are on it. It is level 1 of **Lichess**."]
        )
    );
}

#[tokio::test]
async fn one_override_is_created_on_each_configured_schedule_in_their_order_and_on_no_other() {
    let mut answering = Setup::waking_on("PI7DH85, PFISHNT", three_policies())
        .start()
        .await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    answering
        .creates_overrides(FISHNET_OVERRIDES, &["PQ00002"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    assert_eq!(
        answering.requests().await,
        [
            format!("POST {PRIMARY_OVERRIDES}"),
            format!("POST {FISHNET_OVERRIDES}")
        ]
    );
    let created = answering.posted().await;
    assert_eq!(created[0], created[1]);
    assert_eq!(created[0]["override"]["user"]["id"], "PALICE1");
    assert_eq!(
        reply,
        woken_until(
            &created[0],
            &[
                "- **Primary rota**: you are on it. It is level 1 of **Lichess**, **Tournaments**. Level 1 also pages Dave Example, Erin Example, Frank Example directly.",
                "- **Fishnet rota**: you are on it. It is level 1 of **Fishnet**.",
            ]
        )
    );
}

#[tokio::test]
async fn the_reply_tells_for_each_schedule_whether_the_sender_is_on_it_and_where_it_is_level_one() {
    let mut answering = Setup::waking_on("PBACKUP, PFISHNT, PSPARE1", three_policies())
        .start()
        .await;
    answering
        .creates_overrides(BACKUP_OVERRIDES, &["PQ00001"])
        .await;
    answering.refuses_overrides(FISHNET_OVERRIDES).await;
    answering
        .creates_overrides(SPARE_OVERRIDES, &["PQ00002"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    let created = answering.posted().await;
    assert_eq!(
        reply,
        woken_until(
            &created[0],
            &[
                "- **Backup rota**: you are on it, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first.",
                &format!("- **Fishnet rota**: I couldn't override it: {FORBIDDEN}"),
                "- `PSPARE1`: you are on it, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first.",
            ]
        )
    );
}

#[tokio::test]
async fn when_no_override_could_be_created_the_reply_promises_nothing() {
    let mut answering = Setup::waking_on("PI7DH85, PSPARE1", lichess_alone())
        .start()
        .await;
    answering.refuses_overrides(PRIMARY_OVERRIDES).await;
    answering.refuses_overrides(SPARE_OVERRIDES).await;

    assert_eq!(
        answering
            .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
            .await,
        format!(
            ":warning: I couldn't put you on any schedule.\n\
             - **Primary rota**: I couldn't override it: {FORBIDDEN}\n\
             - `PSPARE1`: I couldn't override it: {FORBIDDEN}"
        )
    );
}

#[tokio::test]
async fn an_override_whose_answer_was_lost_or_unreadable_may_exist_and_the_reply_says_so() {
    for (answer, error) in [
        (
            ResponseTemplate::new(500),
            "pagerduty rejected the request (500 Internal Server Error): Internal Server Error",
        ),
        (
            ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"),
            "pagerduty rejected the request (502 Bad Gateway): Bad Gateway",
        ),
        (
            ResponseTemplate::new(201).set_body_json(json!({ "overrides": [] })),
            "pagerduty answered 201 Created with a body of an unexpected shape",
        ),
        (
            ResponseTemplate::new(200).set_body_string("ok"),
            "pagerduty answered 200 OK with a body of an unexpected shape",
        ),
    ] {
        let mut answering = Setup::waking_on_both_rotas().start().await;
        answers_a_create(&answering, FISHNET_OVERRIDES, answer).await;
        answering
            .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
            .await;

        let reply = answering
            .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
            .await;

        let created = answering.posted().await;
        assert_eq!(
            reply,
            woken_until(
                &created[0],
                &[
                    &format!(
                        "- **Fishnet rota**: I couldn't confirm the override: {error}. It may exist; check PagerDuty."
                    ),
                    "- **Primary rota**: you are on it. It is level 1 of **Lichess**.",
                ]
            )
        );
    }
}

#[tokio::test]
async fn an_override_that_pagerduty_never_answered_may_exist_and_the_reply_says_so() {
    let mut answering = Setup::default().start().await;
    let never = ResponseTemplate::new(201).set_delay(Duration::from_hours(1));
    answers_a_create(&answering, PRIMARY_OVERRIDES, never).await;
    tokio::time::pause();

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    assert_eq!(
        reply,
        ":warning: I couldn't put you on any schedule.\n\
         - **Primary rota**: I couldn't confirm the override: pagerduty request failed. It may exist; check PagerDuty."
    );
}

#[tokio::test]
async fn an_override_that_pagerduty_refused_with_a_client_error_does_not_exist() {
    for (status, error) in [
        (
            400,
            "pagerduty rejected the request (400 Bad Request): Bad Request",
        ),
        (
            404,
            "pagerduty rejected the request (404 Not Found): Not Found",
        ),
        (
            429,
            "pagerduty rejected the request (429 Too Many Requests): Too Many Requests",
        ),
    ] {
        let mut answering = Setup::default().start().await;
        answers_a_create(&answering, PRIMARY_OVERRIDES, ResponseTemplate::new(status)).await;

        assert_eq!(
            answering
                .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
                .await,
            format!(
                ":warning: I couldn't put you on any schedule.\n\
                 - **Primary rota**: I couldn't override it: {error}"
            )
        );
    }
}

#[tokio::test]
async fn with_no_policy_in_use_the_configured_schedule_is_still_overridden() {
    let mut answering = Setup::with_roster(Roster { policies: vec![] })
        .start()
        .await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    let created = answering.posted().await;
    assert_eq!(
        reply,
        woken_until(
            &created[0],
            &[
                "- `PI7DH85`: you are on it, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first."
            ]
        )
    );
}

#[tokio::test]
async fn without_a_configured_schedule_waking_is_off_whoever_asks_and_whatever_is_read() {
    for setup in [
        Setup::default(),
        Setup {
            roster: None,
            ..Setup::default()
        },
        Setup {
            directory: None,
            ..Setup::default()
        },
    ] {
        let setup = Setup {
            config: Config::default(),
            ..setup
        };
        let mut answering = setup.start().await;

        for (sender, text) in [
            (ALICE, "wake me for 4 hours"),
            (ALICE, "wake me"),
            (ALICE, "wake me for 8 days"),
            (STRANGER, "wake me for 4 hours"),
        ] {
            assert_eq!(
                answering.reply(&in_sysadmin(sender, text)).await,
                WAKING_IS_OFF,
                "{text}"
            );
        }
        did_nothing(&answering).await;
    }
}

#[tokio::test]
async fn a_sender_who_is_no_known_pagerduty_user_cannot_be_woken() {
    let mut answering = Setup::default().start().await;

    for text in ["wake me for 4 hours", "wake me"] {
        assert_eq!(
            answering.reply(&in_sysadmin(STRANGER, text)).await,
            "I don't know which PagerDuty user you are. I match people by email address, then by full name. Ask whoever runs wokesy to link your accounts.",
            "{text}"
        );
    }
    did_nothing(&answering).await;
}

#[tokio::test]
async fn wake_me_without_a_duration_asks_for_how_long() {
    let mut answering = Setup::default().start().await;

    for text in ["wake me", "wake me for anything", "wake me for 0 hours"] {
        assert_eq!(
            answering.reply(&in_sysadmin(ALICE, text)).await,
            "For how long? For example: `wake me for 4 hours`.",
            "{text}"
        );
    }
    did_nothing(&answering).await;
}

#[tokio::test]
async fn a_duration_longer_than_the_longest_allowed_is_refused() {
    let mut answering = Setup::default().start().await;

    for text in [
        "wake me for 8 days",
        "wake me for 7 days 1 minute",
        "wake me for 99999999999999999999999 days",
    ] {
        assert_eq!(
            answering.reply(&in_sysadmin(ALICE, text)).await,
            "That is longer than I'm allowed to set (7days). Ask for less, or change the schedule in PagerDuty.",
            "{text}"
        );
    }
    did_nothing(&answering).await;
}

#[tokio::test]
async fn the_longest_duration_allowed_is_configured_and_can_be_asked_for_exactly() {
    let setup = Setup {
        config: Config {
            longest: Duration::from_hours(2),
            ..waking_on("PI7DH85")
        },
        ..Setup::default()
    };
    let mut answering = setup.start().await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;

    assert_eq!(
        answering
            .reply(&in_sysadmin(ALICE, "wake me for 2h1m"))
            .await,
        "That is longer than I'm allowed to set (2h). Ask for less, or change the schedule in PagerDuty."
    );
    assert!(
        answering
            .reply(&in_sysadmin(ALICE, "wake me for 2 hours"))
            .await
            .starts_with(":alarm_clock:")
    );
}

#[tokio::test]
async fn the_roster_is_asked_to_refresh_once_overrides_were_tried() {
    for refused in [false, true] {
        let mut answering = Setup::waking_on_both_rotas().start().await;
        if refused {
            answering.refuses_overrides(FISHNET_OVERRIDES).await;
            answering.refuses_overrides(PRIMARY_OVERRIDES).await;
        } else {
            answering
                .creates_overrides(FISHNET_OVERRIDES, &["PQ00001"])
                .await;
            answering
                .creates_overrides(PRIMARY_OVERRIDES, &["PQ00002"])
                .await;
        }

        answering
            .answer(&in_sysadmin(ALICE, "wake me for 4 hours"))
            .await;

        assert!(answering.was_asked_to_refresh().await, "refused: {refused}");
    }
}

fn woken_by_a_shift_until(created: &Value, lines: &[&str]) -> String {
    format!(
        ":alarm_clock: I'll wake you for anything until <time:{}>.\n{}\n{SHIFT_ENDS}",
        created["custom_shifts"][0]["end_time"].as_str().unwrap(),
        lines.join("\n")
    )
}

#[tokio::test]
async fn on_a_shift_based_schedule_a_custom_shift_is_created_for_the_same_window_instead() {
    let mut answering = Setup::default().start().await;
    answering.is_shift_based(PRIMARY_OVERRIDES).await;
    answering
        .creates_custom_shifts(PRIMARY_SHIFTS, &["PCS0001"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(BOB, "wake me for 90 minutes"))
        .await;

    assert_eq!(
        answering.requests().await,
        [
            format!("POST {PRIMARY_OVERRIDES}"),
            format!("POST {PRIMARY_SHIFTS}")
        ]
    );
    let posted = answering.posted().await;
    let (refused, created) = (&posted[0]["override"], &posted[1]["custom_shifts"]);
    assert_eq!(
        created,
        &json!([{
            "type": "custom_shift",
            "start_time": refused["start"],
            "end_time": refused["end"],
            "assignments": [{
                "type": "shift_assignment",
                "member": { "type": "user_member", "user_id": "PBOB001" },
            }],
        }])
    );
    assert_eq!(
        at(&refused["end"]) - at(&refused["start"]),
        Duration::from_mins(90)
    );
    assert_eq!(
        reply,
        woken_by_a_shift_until(
            &posted[1],
            &[
                "- **Primary rota**: you are on it, alongside whoever the schedule has. It is level 1 of **Lichess**."
            ]
        )
    );
    assert!(answering.was_asked_to_refresh().await);
}

#[tokio::test]
async fn a_schedule_found_to_be_shift_based_is_not_asked_for_an_override_again() {
    let mut answering = Setup::waking_on_both_rotas().start().await;
    answering
        .creates_overrides(FISHNET_OVERRIDES, &["PQ00001", "PQ00002"])
        .await;
    answering.is_shift_based(PRIMARY_OVERRIDES).await;
    answering
        .creates_custom_shifts(PRIMARY_SHIFTS, &["PCS0001", "PCS0002"])
        .await;

    let first = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;
    let second = answering
        .reply(&in_sysadmin(BOB, "wake me for 4 hours"))
        .await;

    assert_eq!(
        answering.requests().await,
        [
            format!("POST {FISHNET_OVERRIDES}"),
            format!("POST {PRIMARY_OVERRIDES}"),
            format!("POST {PRIMARY_SHIFTS}"),
            format!("POST {FISHNET_OVERRIDES}"),
            format!("POST {PRIMARY_SHIFTS}"),
        ]
    );
    let posted = answering.posted().await;
    let lines = [
        "- **Fishnet rota**: you are on it. It is level 1 of **Fishnet**.",
        "- **Primary rota**: you are on it, alongside whoever the schedule has. It is level 1 of **Lichess**.",
    ];
    assert_eq!(first, woken_until(&posted[0], &lines));
    assert_eq!(second, woken_until(&posted[3], &lines));
    assert_eq!(
        posted[4]["custom_shifts"][0]["assignments"][0]["member"]["user_id"],
        "PBOB001"
    );
}

#[tokio::test]
async fn on_a_shift_based_schedule_the_reply_tells_where_it_is_level_one_without_implying_a_swap() {
    let mut answering = Setup::waking_on("PI7DH85, PSPARE1", three_policies())
        .start()
        .await;
    answering.is_shift_based(PRIMARY_OVERRIDES).await;
    answering.is_shift_based(SPARE_OVERRIDES).await;
    answering
        .creates_custom_shifts(PRIMARY_SHIFTS, &["PCS0001"])
        .await;
    answering
        .creates_custom_shifts(SPARE_SHIFTS, &["PCS0002"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    let posted = answering.posted().await;
    assert_eq!(
        reply,
        woken_by_a_shift_until(
            &posted[1],
            &[
                "- **Primary rota**: you are on it, alongside whoever the schedule has. It is level 1 of **Lichess**, **Tournaments**. Level 1 also pages Dave Example, Erin Example, Frank Example directly.",
                "- `PSPARE1`: you are on it, alongside whoever the schedule has, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first.",
            ]
        )
    );
}

#[tokio::test]
async fn a_custom_shift_that_pagerduty_refused_does_not_exist_and_one_whose_answer_was_lost_may() {
    for (answer, line) in [
        (
            ResponseTemplate::new(403)
                .set_body_json(json!({ "error": { "message": "Forbidden", "code": 2010 } })),
            format!("I couldn't add a shift to it: {FORBIDDEN}"),
        ),
        (
            ResponseTemplate::new(400),
            "I couldn't add a shift to it: pagerduty rejected the request (400 Bad Request): Bad Request".to_owned(),
        ),
        (
            ResponseTemplate::new(500),
            "I couldn't confirm the shift: pagerduty rejected the request (500 Internal Server Error): Internal Server Error. It may exist; check PagerDuty.".to_owned(),
        ),
        (
            ResponseTemplate::new(201).set_body_json(json!({ "custom_shifts": [] })),
            "I couldn't confirm the shift: pagerduty answered 201 Created with a body of an unexpected shape. It may exist; check PagerDuty.".to_owned(),
        ),
    ] {
        let mut answering = Setup::default().start().await;
        answering.is_shift_based(PRIMARY_OVERRIDES).await;
        answers_a_create(&answering, PRIMARY_SHIFTS, answer).await;

        assert_eq!(
            answering
                .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
                .await,
            format!(":warning: I couldn't put you on any schedule.\n- **Primary rota**: {line}")
        );
        assert!(answering.was_asked_to_refresh().await);
    }
}
