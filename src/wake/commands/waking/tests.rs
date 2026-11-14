use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use super::*;
use crate::on_call::Roster;
use crate::wake::Config;
use crate::wake::testing::{
    ALICE, Answering, BOB, FISHNET_OVERRIDES, PRIMARY_OVERRIDES, STRANGER, Setup,
    fishnet_and_lichess, fishnet_rota, in_sysadmin, level, policy, primary_rota, schedule, user,
};

const TOURNAMENT_OVERRIDES: &str = "/schedules/PTOURNA/overrides";

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
        format!(
            ":alarm_clock: I'll wake you for anything until <time:{}>.\n\
             - **Lichess**: you are first in line through **Primary rota**.\n\
             PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.",
            shift["end"].as_str().unwrap()
        )
    );
}

#[tokio::test]
async fn one_override_is_created_on_each_distinct_schedule_at_the_first_line_only() {
    let backup_rota = schedule("PBACKUP", "Backup rota");
    let roster = Roster {
        policies: vec![
            policy("PFISHNE", "Fishnet", &[level(1, &[fishnet_rota()], &[])]),
            policy(
                "PLICHES",
                "Lichess",
                &[
                    level(1, &[primary_rota()], &[]),
                    level(2, &[backup_rota], &[]),
                ],
            ),
            policy(
                "PTOURNA",
                "Tournaments",
                &[level(1, &[primary_rota()], &[])],
            ),
        ],
    };
    let mut answering = Setup::with_roster(roster).start().await;
    answering
        .creates_overrides(FISHNET_OVERRIDES, &["PQ00001"])
        .await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00002"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    assert_eq!(
        answering.requests().await,
        [
            format!("POST {FISHNET_OVERRIDES}"),
            format!("POST {PRIMARY_OVERRIDES}")
        ]
    );
    let created = answering.posted().await;
    assert_eq!(created[0], created[1]);
    assert_eq!(created[0]["override"]["user"]["id"], "PALICE1");
    assert_eq!(
        reply,
        format!(
            ":alarm_clock: I'll wake you for anything until <time:{}>.\n\
             - **Fishnet**: you are first in line through **Fishnet rota**.\n\
             - **Lichess**: you are first in line through **Primary rota**.\n\
             - **Tournaments**: you are first in line through **Primary rota**.\n\
             PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.",
            created[0]["override"]["end"].as_str().unwrap()
        )
    );
}

#[tokio::test]
async fn the_reply_tells_for_each_policy_whether_the_sender_is_first_in_line() {
    let dave = user("PDAVE01", "Dave Example");
    let erin = user("PERIN01", "Erin Example");
    let roster = Roster {
        policies: vec![
            policy("PFISHNE", "Fishnet", &[level(1, &[fishnet_rota()], &[])]),
            policy(
                "PLICHES",
                "Lichess",
                &[level(1, &[primary_rota()], &[dave.clone(), erin])],
            ),
            policy("PMODERA", "Moderation", &[level(1, &[], &[dave])]),
            policy("PNOBODY", "Nobody yet", &[]),
            policy(
                "PTOURNA",
                "Tournaments",
                &[level(1, &[schedule("PTOURNA", "Tournament rota")], &[])],
            ),
        ],
    };
    let mut answering = Setup::with_roster(roster).start().await;
    answering.refuses_overrides(FISHNET_OVERRIDES).await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    answering
        .creates_overrides(TOURNAMENT_OVERRIDES, &["PQ00002"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    let created = answering.posted().await;
    assert_eq!(
        reply,
        format!(
            ":alarm_clock: I'll wake you for anything until <time:{}>.\n\
             - **Fishnet**: I couldn't override **Fishnet rota**: pagerduty rejected the request (403 Forbidden): Forbidden\n\
             - **Lichess**: you are first in line through **Primary rota**. Level 1 also pages Dave Example, Erin Example directly.\n\
             - **Moderation**: I can't put you first. Level 1 has no schedule to override.\n\
             - **Nobody yet**: I can't put you first. Level 1 has no schedule to override.\n\
             - **Tournaments**: you are first in line through **Tournament rota**.\n\
             PagerDuty hands it back to the schedule by itself when the time is up. Send `stop waking me` to end it sooner.",
            created[1]["override"]["end"].as_str().unwrap()
        )
    );
}

#[tokio::test]
async fn a_policy_with_several_schedules_at_the_first_line_tells_which_were_overridden() {
    let roster = Roster {
        policies: vec![policy(
            "PLICHES",
            "Lichess",
            &[level(
                1,
                &[
                    primary_rota(),
                    fishnet_rota(),
                    schedule("PTOURNA", "Tournament rota"),
                ],
                &[user("PDAVE01", "Dave Example")],
            )],
        )],
    };
    let mut answering = Setup::with_roster(roster).start().await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    answering.refuses_overrides(FISHNET_OVERRIDES).await;
    answering
        .creates_overrides(TOURNAMENT_OVERRIDES, &["PQ00002"])
        .await;

    let reply = answering
        .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
        .await;

    let line = reply.lines().nth(1).unwrap();
    assert_eq!(
        line,
        "- **Lichess**: you are first in line through **Primary rota**, **Tournament rota**. Level 1 also pages Dave Example directly. I couldn't override **Fishnet rota**: pagerduty rejected the request (403 Forbidden): Forbidden"
    );
}

#[tokio::test]
async fn when_no_override_could_be_created_the_reply_promises_nothing() {
    let roster = Roster {
        policies: vec![
            policy(
                "PLICHES",
                "Lichess",
                &[level(
                    1,
                    &[primary_rota()],
                    &[user("PDAVE01", "Dave Example")],
                )],
            ),
            policy("PMODERA", "Moderation", &[level(1, &[], &[])]),
        ],
    };
    let mut answering = Setup::with_roster(roster).start().await;
    answering.refuses_overrides(PRIMARY_OVERRIDES).await;

    assert_eq!(
        answering
            .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
            .await,
        ":warning: I couldn't put you first anywhere.\n\
         - **Lichess**: I couldn't override **Primary rota**: pagerduty rejected the request (403 Forbidden): Forbidden\n\
         - **Moderation**: I can't put you first. Level 1 has no schedule to override."
    );
}

#[tokio::test]
async fn with_no_schedule_at_any_first_line_nothing_is_asked_of_pagerduty() {
    let roster = Roster {
        policies: vec![policy(
            "PMODERA",
            "Moderation",
            &[level(1, &[], &[user("PDAVE01", "Dave Example")])],
        )],
    };
    let mut answering = Setup::with_roster(roster).start().await;

    assert_eq!(
        answering
            .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
            .await,
        ":warning: I couldn't put you first anywhere.\n\
         - **Moderation**: I can't put you first. Level 1 has no schedule to override."
    );
    assert_eq!(answering.requests().await, Vec::<String>::new());
}

#[tokio::test]
async fn with_no_policy_in_use_there_is_nothing_to_wake_the_sender_for() {
    let mut answering = Setup::with_roster(Roster { policies: vec![] })
        .start()
        .await;

    assert_eq!(
        answering
            .reply(&in_sysadmin(ALICE, "wake me for 4 hours"))
            .await,
        "No escalation policy is used by a service, so there is nothing to wake you for."
    );
    did_nothing(&answering).await;
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
            ..Config::default()
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
        let mut answering = Setup::with_roster(fishnet_and_lichess()).start().await;
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
