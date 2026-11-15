use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::on_call::Roster;
use crate::pagerduty::UserRef;
use crate::wake::Config;
use crate::wake::testing::{
    ALICE, Answering, BOB, FISHNET_OVERRIDES, PRIMARY_OVERRIDES, PRIMARY_SHIFTS, STRANGER, Setup,
    as_a_message, crowd, fishnet_rota, in_sysadmin, level, lichess_alone, on_call_at, only_policy,
    policy, primary_rota, schedule, user, waking_on, with_on_call,
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

    let otherwise = answering
        .acknowledged(&in_sysadmin(BOB, "wake me for 90 minutes"))
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
        otherwise,
        ":alarm_clock: @_**|13** is now on call for 1 hour 30 minutes. Level 2 (1 person) is backup."
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
            .acknowledged(&in_sysadmin(ALICE, "wake me for 2 hours"))
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

    let otherwise = answering
        .acknowledged(&in_sysadmin(BOB, "wake me for 90 minutes"))
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
        otherwise,
        ":alarm_clock: @_**|13** is now on call for 1 hour 30 minutes. Level 2 (1 person) is backup."
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

const FOUR_HOURS: &str = ":alarm_clock: @_**|12** is now on call for 4 hours";

fn alice() -> UserRef {
    user("PALICE1", "Alice Example")
}

fn both_rotas_at_level_one() -> Roster {
    Roster {
        policies: vec![policy(
            "PLICHES",
            "Lichess",
            &[level(1, &[primary_rota(), fishnet_rota()], &[])],
        )],
    }
}

async fn alice_wakes_for_4_hours(answering: &mut Answering) -> Reply {
    let answer = answering
        .answer(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;
    assert!(answering.was_asked_to_refresh().await);
    assert_eq!(answer.announcement, None);
    answer.reply
}

async fn alice_wakes_by_an_override_with(roster: Roster) -> Reply {
    let mut answering = Setup::with_roster(roster).start().await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    alice_wakes_for_4_hours(&mut answering).await
}

async fn alice_wakes_by_a_custom_shift_with(roster: Roster) -> Reply {
    let mut answering = Setup::with_roster(roster).start().await;
    answering.is_shift_based(PRIMARY_OVERRIDES).await;
    answering
        .creates_custom_shifts(PRIMARY_SHIFTS, &["PCS0001"])
        .await;
    alice_wakes_for_4_hours(&mut answering).await
}

#[tokio::test]
async fn a_sender_put_alone_on_level_one_of_the_only_policy_is_acknowledged_instead_of_the_one_line()
 {
    let roster = only_policy(&[
        level(1, &[primary_rota()], &[]),
        on_call_at(2, &crowd(3)),
        on_call_at(3, &crowd(3)),
    ]);
    let one_line = format!("{FOUR_HOURS}. Level 2 (3 people) and Level 3 (3 people) are backup.");

    assert_eq!(
        alice_wakes_by_an_override_with(roster.clone()).await,
        Reply::acknowledgement(one_line.clone())
    );
    assert_eq!(
        alice_wakes_by_a_custom_shift_with(roster).await,
        Reply::acknowledgement(one_line)
    );
}

#[tokio::test]
async fn with_nobody_behind_the_sender_the_one_line_ends_after_the_duration() {
    let roster = only_policy(&[level(1, &[primary_rota()], &[]), on_call_at(2, &[])]);

    assert_eq!(
        alice_wakes_by_an_override_with(roster).await,
        Reply::acknowledgement(format!("{FOUR_HOURS}."))
    );
}

#[tokio::test]
async fn on_a_shift_based_schedule_others_on_call_at_level_one_are_counted_in_a_message() {
    for (on_call, together) in [
        (crowd(5), ", together with 5 others at level 1"),
        (crowd(1), ", together with 1 other at level 1"),
        (
            vec![alice(), user("PDAVE01", "Dave Example")],
            ", together with 1 other at level 1",
        ),
    ] {
        let roster = only_policy(&[
            with_on_call(level(1, &[primary_rota()], &[]), &on_call),
            on_call_at(2, &crowd(3)),
        ]);

        assert_eq!(
            alice_wakes_by_a_custom_shift_with(roster).await,
            Reply::message(format!(
                "{FOUR_HOURS}{together}. Level 2 (3 people) is backup."
            ))
        );
    }
}

#[tokio::test]
async fn on_a_shift_based_schedule_a_sender_already_alone_at_level_one_is_acknowledged() {
    let roster = only_policy(&[
        with_on_call(level(1, &[primary_rota()], &[]), &[alice()]),
        on_call_at(2, &crowd(3)),
    ]);

    assert_eq!(
        alice_wakes_by_a_custom_shift_with(roster).await,
        Reply::acknowledgement(format!("{FOUR_HOURS}. Level 2 (3 people) is backup."))
    );
}

#[tokio::test]
async fn on_a_legacy_schedule_who_was_scheduled_is_replaced_and_only_who_is_paged_directly_is_counted()
 {
    let dave = user("PDAVE01", "Dave Example");
    let scheduled = user("PBOB001", "Bob Example");
    let replaced = only_policy(&[with_on_call(
        level(1, &[primary_rota()], &[]),
        std::slice::from_ref(&scheduled),
    )]);
    let beside_dave = only_policy(&[with_on_call(
        level(1, &[primary_rota()], &[dave.clone(), alice()]),
        &[scheduled, dave, alice()],
    )]);

    assert_eq!(
        alice_wakes_by_an_override_with(replaced).await,
        Reply::acknowledgement(format!("{FOUR_HOURS}."))
    );
    assert_eq!(
        alice_wakes_by_an_override_with(beside_dave).await,
        Reply::message(format!("{FOUR_HOURS}, together with 1 other at level 1."))
    );
}

#[tokio::test]
async fn the_one_line_tells_the_duration_in_words() {
    for (asked, told) in [
        ("wake me for 30 minutes", "30 minutes"),
        ("wake me for an hour", "1 hour"),
        ("wake me for 4h", "4 hours"),
        ("wake me for 90 minutes", "1 hour 30 minutes"),
        ("wake me for 1 day", "1 day"),
        ("wake me for 2 days", "2 days"),
        ("wake me for 1d 2h 5m", "1 day 2 hours 5 minutes"),
    ] {
        let mut answering = Setup::default().start().await;
        answering
            .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
            .await;

        assert_eq!(
            answering.acknowledged(&in_sysadmin(ALICE, asked)).await,
            format!(
                ":alarm_clock: @_**|12** is now on call for {told}. Level 2 (1 person) is backup."
            ),
            "{asked}"
        );
    }
}

#[tokio::test]
async fn when_every_schedule_is_at_level_one_of_the_only_policy_and_changed_alike_the_reply_is_one_line()
 {
    let mut answering = Setup::waking_on("PI7DH85, PFISHNT", both_rotas_at_level_one())
        .start()
        .await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    answering
        .creates_overrides(FISHNET_OVERRIDES, &["PQ00002"])
        .await;

    assert_eq!(
        alice_wakes_for_4_hours(&mut answering).await,
        Reply::acknowledgement(format!("{FOUR_HOURS}."))
    );
}

#[tokio::test]
async fn a_schedule_that_could_not_be_changed_keeps_the_reply_for_each_schedule() {
    for (answer, outcome) in [
        (
            ResponseTemplate::new(403)
                .set_body_json(json!({ "error": { "message": "Forbidden", "code": 2010 } })),
            format!("I couldn't override it: {FORBIDDEN}"),
        ),
        (
            ResponseTemplate::new(500),
            "I couldn't confirm the override: pagerduty rejected the request (500 Internal Server Error): Internal Server Error. It may exist; check PagerDuty.".to_owned(),
        ),
    ] {
        let mut answering = Setup::waking_on("PI7DH85, PFISHNT", both_rotas_at_level_one())
            .start()
            .await;
        answering
            .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
            .await;
        answers_a_create(&answering, FISHNET_OVERRIDES, answer).await;

        let reply = as_a_message(alice_wakes_for_4_hours(&mut answering).await);

        let created = answering.posted().await;
        assert_eq!(
            reply,
            woken_until(
                &created[0],
                &[
                    "- **Primary rota**: you are on it. It is level 1 of **Lichess**.",
                    &format!("- **Fishnet rota**: {outcome}"),
                ]
            )
        );
    }
}

#[tokio::test]
async fn a_schedule_that_is_not_at_level_one_keeps_the_reply_for_each_schedule() {
    let mut answering = Setup::waking_on("PI7DH85, PSPARE1", lichess_alone())
        .start()
        .await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    answering
        .creates_overrides(SPARE_OVERRIDES, &["PQ00002"])
        .await;

    let reply = as_a_message(alice_wakes_for_4_hours(&mut answering).await);

    let created = answering.posted().await;
    assert_eq!(
        reply,
        woken_until(
            &created[0],
            &[
                "- **Primary rota**: you are on it. It is level 1 of **Lichess**.",
                "- `PSPARE1`: you are on it, but it is not at level 1 of any escalation policy in use, so alerts may not reach you first.",
            ]
        )
    );
}

#[tokio::test]
async fn schedules_of_both_kinds_at_level_one_keep_the_reply_for_each_schedule() {
    let mut answering = Setup::waking_on("PI7DH85, PFISHNT", both_rotas_at_level_one())
        .start()
        .await;
    answering.is_shift_based(PRIMARY_OVERRIDES).await;
    answering
        .creates_custom_shifts(PRIMARY_SHIFTS, &["PCS0001"])
        .await;
    answering
        .creates_overrides(FISHNET_OVERRIDES, &["PQ00001"])
        .await;

    let reply = as_a_message(alice_wakes_for_4_hours(&mut answering).await);

    let posted = answering.posted().await;
    assert_eq!(
        reply,
        woken_until(
            &posted[2],
            &[
                "- **Primary rota**: you are on it, alongside whoever the schedule has. It is level 1 of **Lichess**.",
                "- **Fishnet rota**: you are on it. It is level 1 of **Lichess**.",
            ]
        )
    );
}

#[tokio::test]
async fn an_override_beside_a_schedule_wokesy_does_not_change_keeps_the_reply_for_each_schedule() {
    let reply = as_a_message(alice_wakes_by_an_override_with(both_rotas_at_level_one()).await);

    assert!(
        reply.ends_with(&format!(
            "\n- **Primary rota**: you are on it. It is level 1 of **Lichess**.\n{HANDED_BACK}"
        )),
        "{reply}"
    );
    assert!(
        reply.starts_with(":alarm_clock: I'll wake you for anything until <time:"),
        "{reply}"
    );
}
