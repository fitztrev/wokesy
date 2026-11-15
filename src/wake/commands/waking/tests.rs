use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use super::*;
use crate::on_call::Roster;
use crate::wake::Config;
use crate::wake::testing::{
    ALICE, Answering, BOB, FISHNET_OVERRIDES, PRIMARY_OVERRIDES, STRANGER, Setup, fishnet_rota,
    in_sysadmin, level, lichess_alone, policy, primary_rota, schedule, user, waking_on,
};

const BACKUP_OVERRIDES: &str = "/schedules/PBACKUP/overrides";
const SPARE_OVERRIDES: &str = "/schedules/PSPARE1/overrides";
const WAKING_IS_OFF: &str = "Waking is not set up here. Change the schedule in PagerDuty directly.";
const HANDED_BACK: &str = "PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.";
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
