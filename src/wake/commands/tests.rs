use super::*;
use crate::on_call::Roster;
use crate::wake::testing::{
    ALICE, Answering, BOB, ON_CALL_NOW, PRIMARY_OVERRIDES, STRANGER, Setup, channel, direct,
    directory, in_sysadmin, level, mention_in, policy, schedule, user, with_on_call,
};

const EVERY_COMMAND: [&str; 7] = [
    "page lichess.org is down",
    "wake me for 4 hours",
    "stop waking me",
    "back to defaults",
    "who is on call",
    "help",
    "good morning",
];

async fn open_in_sysadmin_and_on_call() -> Answering {
    let setup = Setup {
        open_channels: vec![channel("sysadmin"), channel("on-call")],
        ..Setup::default()
    };
    setup.start().await
}

async fn did_nothing(answering: &Answering) {
    assert_eq!(answering.requests().await, Vec::<String>::new());
    assert!(!answering.was_asked_to_refresh().await);
}

#[tokio::test]
async fn a_direct_message_is_pointed_to_the_open_channels_whatever_it_asks() {
    let mut answering = open_in_sysadmin_and_on_call().await;

    for command in EVERY_COMMAND {
        assert_eq!(
            answering.answer(&direct(ALICE, command)).await,
            Answer::only(
                "I only take commands in the open, in #**sysadmin** or #**on-call**. Mention me there."
            ),
            "{command}"
        );
    }

    did_nothing(&answering).await;
}

#[tokio::test]
async fn a_mention_in_a_channel_that_is_not_open_is_pointed_to_the_open_ones_whatever_it_asks() {
    let mut answering = open_in_sysadmin_and_on_call().await;

    for command in EVERY_COMMAND {
        assert_eq!(
            answering
                .answer(&mention_in("general", ALICE, command))
                .await,
            Answer::only(
                "I only take commands in the open, in #**sysadmin** or #**on-call**. Mention me there."
            ),
            "{command}"
        );
    }

    did_nothing(&answering).await;
}

#[tokio::test]
async fn a_channel_is_open_only_under_its_exact_name() {
    let mut answering = Setup::default().start().await;

    for channel in ["Sysadmin", "sysadmin-team", "sys"] {
        assert_eq!(
            answering.reply(&mention_in(channel, ALICE, "help")).await,
            "I only take commands in the open, in #**sysadmin**. Mention me there.",
            "{channel}"
        );
    }
}

#[tokio::test]
async fn a_mention_in_any_open_channel_is_answered() {
    let mut answering = open_in_sysadmin_and_on_call().await;

    for channel in ["sysadmin", "on-call"] {
        assert_eq!(
            answering.reply(&mention_in(channel, ALICE, "help")).await,
            HELP,
            "{channel}"
        );
    }
}

#[tokio::test]
async fn with_no_channels_configured_only_the_announcements_channel_is_open() {
    let setup = Setup {
        open_channels: Config::default().open_channels(&channel("announcements")),
        ..Setup::default()
    };
    let mut answering = setup.start().await;

    assert_eq!(
        answering
            .reply(&mention_in("announcements", ALICE, "help"))
            .await,
        HELP
    );
    assert_eq!(
        answering.reply(&in_sysadmin(ALICE, "help")).await,
        "I only take commands in the open, in #**announcements**. Mention me there."
    );
}

#[tokio::test]
async fn help_lists_the_commands() {
    let mut answering = Setup::default().start().await;

    assert_eq!(
        answering.answer(&in_sysadmin(STRANGER, "help")).await,
        Answer::only(
            "I manage who is on call in PagerDuty.\n\
             - `page lichess.org is down` alerts whoever is on call, through PagerDuty. Anyone here can use it. After a page I wait a while before I page again.\n\
             - `wake me for 4 hours` puts you on the wake schedule for that long. PagerDuty ends it by itself when the time is up.\n\
             - `stop waking me` ends that early.\n\
             - `back to defaults` ends every override and shift I made, for you or anyone else, so PagerDuty's own schedules decide again.\n\
             - `who is on call` shows who is on call now."
        )
    );
}

#[tokio::test]
async fn an_unknown_command_is_pointed_to_help() {
    let mut answering = Setup::default().start().await;

    assert_eq!(
        answering.answer(&in_sysadmin(ALICE, "good morning")).await,
        Answer::only("I don't know that command. Send `help` to see the commands I know.")
    );
}

#[tokio::test]
async fn who_shows_who_is_on_call_now_to_anyone() {
    let mut answering = Setup::default().start().await;

    for sender in [BOB, STRANGER] {
        assert_eq!(
            answering
                .answer(&in_sysadmin(sender, "who is on call?"))
                .await,
            Answer::only(ON_CALL_NOW)
        );
    }

    did_nothing(&answering).await;
}

#[tokio::test]
async fn commands_that_need_the_roster_and_the_directory_wait_until_both_are_read() {
    for setup in [
        Setup {
            roster: None,
            ..Setup::default()
        },
        Setup {
            directory: None,
            ..Setup::default()
        },
    ] {
        let mut answering = setup.start().await;

        for command in [
            "wake me for 4 hours",
            "stop waking me",
            "back to defaults",
            "who is on call",
        ] {
            assert_eq!(
                answering.answer(&in_sysadmin(ALICE, command)).await,
                Answer::only("I'm still reading PagerDuty. Try again in a minute."),
                "{command}"
            );
        }
        assert_eq!(answering.reply(&in_sysadmin(ALICE, "help")).await, HELP);
        assert_eq!(
            answering.reply(&in_sysadmin(ALICE, "good morning")).await,
            UNRECOGNIZED
        );

        did_nothing(&answering).await;
    }
}

#[tokio::test]
async fn a_command_is_answered_from_the_roster_and_the_directory_as_they_are_when_it_arrives() {
    let setup = Setup {
        roster: None,
        directory: None,
        ..Setup::default()
    };
    let mut answering = setup.start().await;
    assert_eq!(
        answering.reply(&in_sysadmin(ALICE, "who")).await,
        STILL_READING
    );

    answering.roster.send_replace(Setup::default().roster);
    answering.directory.send_replace(Setup::default().directory);

    assert_eq!(
        answering.reply(&in_sysadmin(ALICE, "who")).await,
        ON_CALL_NOW
    );
}

#[tokio::test]
async fn no_reply_repeats_a_name_from_pagerduty_as_a_mention_that_notifies() {
    let rota = schedule("PI7DH85", "@*sysadmins*");
    let carol = user("PCAROL1", "@**Carol Example**");
    let first_line = with_on_call(
        level(1, &[rota], std::slice::from_ref(&carol)),
        std::slice::from_ref(&carol),
    );
    let roster = Roster {
        policies: vec![policy("PLICHES", "x @**all**", &[first_line])],
    };
    let setup = Setup {
        directory: Some(directory(&[("PALICE1", "@**everyone**", Some(ALICE))])),
        ..Setup::with_roster(roster)
    };
    let mut answering = setup.start().await;
    answering.accepts_pages().await;
    answering
        .creates_overrides(PRIMARY_OVERRIDES, &["PQ00001", "PQ00002", "PQ00003"])
        .await;
    answering
        .deletes_overrides(PRIMARY_OVERRIDES, &["PQ00001"])
        .await;
    answering.refuses_overrides(PRIMARY_OVERRIDES).await;

    let mut replies = Vec::new();
    for command in [
        "who is on call",
        "page lichess.org is down",
        "wake me for 4 hours",
        "stop waking me",
        "wake me for 4 hours",
        "wake me for 4 hours",
        "back to defaults",
        "wake me for 4 hours",
    ] {
        replies.push(
            answering
                .said_either_way(&in_sysadmin(ALICE, command))
                .await,
        );
    }

    let said = replies.join("\n");
    for silent in [
        "**x @_**all** · Level 1**",
        "**@_*sysadmins***",
        "@_**Carol Example**",
        "@_**everyone**",
    ] {
        assert!(said.contains(silent), "{silent} is missing from {said}");
    }
    assert!(!said.contains("@*"), "{said}");
    assert!(!said.contains("Waking is not set up"), "{said}");
}
