use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::pagerduty::UserRef;
use crate::wake::Config;
use crate::wake::testing::{
    ALICE, Answering, BOB, ENQUEUE, STRANGER, Setup, crowd, fishnet_and_lichess, in_sysadmin,
    on_call_at, only_policy, user,
};

const PAGED_BOB: &str = ":rotating_light: Paged @_**|13**. Level 2 (1 person) is backup.";
const PAGED_WHOEVER: &str = ":rotating_light: Paged. PagerDuty is alerting whoever is on call.";

async fn paging() -> Answering {
    let answering = Setup::default().start().await;
    answering.accepts_pages().await;
    answering
}

async fn paging_with_cooldown(page_cooldown: Duration) -> Answering {
    let setup = Setup {
        config: Config {
            page_cooldown,
            ..Config::default()
        },
        ..Setup::default()
    };
    let answering = setup.start().await;
    answering.accepts_pages().await;
    answering
}

fn page_by(sender: ChatUserId) -> IncomingMessage {
    in_sysadmin(sender, "page lichess.org is down")
}

fn alice() -> UserRef {
    user("PALICE1", "Alice Example")
}

fn bob() -> UserRef {
    user("PBOB001", "Bob Example")
}

fn carol() -> UserRef {
    user("PCAROL1", "Carol Example")
}

async fn reply_to_a_page_with(roster: Roster) -> String {
    let mut answering = Setup::with_roster(roster).start().await;
    answering.accepts_pages().await;
    let reply = answering.reply(&page_by(ALICE)).await;
    assert_eq!(answering.requests().await, ["POST /v2/enqueue"]);
    reply
}

async fn summaries(answering: &Answering) -> Vec<String> {
    answering
        .posted()
        .await
        .iter()
        .map(|alert| alert["payload"]["summary"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_page_alerts_pagerduty_with_the_senders_name_and_what_is_wrong() {
    let mut answering = Setup::default().start().await;
    Mock::given(method("POST"))
        .and(path(ENQUEUE))
        .and(body_json(json!({
            "routing_key": "samplekeyhere",
            "event_action": "trigger",
            "payload": {
                "summary": "Sam Stranger: Lichess.org is DOWN!",
                "source": "Zulip",
                "severity": "critical",
                "custom_details": { "paged_by": "Sam Stranger" },
            },
        })))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&answering.pagerduty)
        .await;

    let answer = answering
        .answer(&in_sysadmin(STRANGER, "Page Lichess.org is DOWN!"))
        .await;

    assert_eq!(answer.reply.markdown(), PAGED_BOB);
    assert_eq!(answering.requests().await, ["POST /v2/enqueue"]);
}

#[tokio::test]
async fn a_page_is_announced_with_what_is_wrong_only_inside_a_quote() {
    let mut answering = paging().await;

    let answer = answering.answer(&page_by(ALICE)).await;

    assert_eq!(
        answer.announcement.unwrap().markdown(),
        ":rotating_light: @_**|12** paged on call:\n> lichess.org is down"
    );
}

#[tokio::test]
async fn every_line_of_what_is_wrong_is_quoted_and_every_mention_in_it_is_silent() {
    let mut answering = paging().await;
    let text = "page the database is down @**all**\r\n\r\n@**Carol Example** please look\n```\nFATAL: no space left\rPANIC\n\n@*sysadmins*";

    let answer = answering.answer(&in_sysadmin(ALICE, text)).await;

    assert_eq!(
        answer.announcement.unwrap().markdown(),
        ":rotating_light: @_**|12** paged on call:\n\
         > the database is down @_**all**\n\
         > \n\
         > @_**Carol Example** please look\n\
         > ```\n\
         > FATAL: no space left\n\
         > PANIC\n\
         > \n\
         > @_*sysadmins*"
    );
    assert_eq!(answer.reply.markdown(), PAGED_BOB);
}

#[tokio::test]
async fn a_mention_zulip_would_not_silence_inside_the_quote_cannot_notify_either() {
    for mention in ["@**Carol Example**", "@*sysadmins*", "@**all**"] {
        let mut answering = paging().await;
        let text = format!("page >{mention} db is down\n> >{mention}\n>>{mention}");

        let answer = answering.answer(&in_sysadmin(ALICE, &text)).await;

        let silent = mention.replacen('@', "@_", 1);
        assert_eq!(
            answer.announcement.unwrap().markdown(),
            format!(
                ":rotating_light: @_**|12** paged on call:\n\
                 > >{silent} db is down\n\
                 > > >{silent}\n\
                 > >>{silent}"
            ),
            "{mention}"
        );
        assert_eq!(
            summaries(&answering).await,
            [format!(
                "Alice Example: >{mention} db is down\n> >{mention}\n>>{mention}"
            )]
        );
    }
}

#[tokio::test]
async fn a_page_does_not_wait_for_the_roster_or_the_directory() {
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
        answering.accepts_pages().await;

        let answer = answering.answer(&page_by(ALICE)).await;

        assert_eq!(answer.reply.markdown(), PAGED_WHOEVER);
        assert!(answer.announcement.is_some());
        assert_eq!(
            summaries(&answering).await,
            ["Alice Example: lichess.org is down"]
        );
    }
}

#[tokio::test]
async fn a_summary_is_cut_to_the_1024_characters_pagerduty_allows() {
    let mut answering = paging().await;
    let long = "é".repeat(2000);

    let answer = answering
        .answer(&in_sysadmin(ALICE, &format!("page {long}")))
        .await;

    let cut = format!(
        "Alice Example: {}",
        "é".repeat(1024 - "Alice Example: ".len())
    );
    assert_eq!(summaries(&answering).await, [cut]);
    assert_eq!(
        answer.announcement.unwrap().markdown(),
        format!(":rotating_light: @_**|12** paged on call:\n> {long}")
    );
}

#[tokio::test]
async fn without_a_routing_key_paging_is_off() {
    let setup = Setup {
        paging: false,
        ..Setup::default()
    };
    let mut answering = setup.start().await;

    assert_eq!(
        answering.answer(&page_by(ALICE)).await,
        Answer::only("Paging is not set up here. Use PagerDuty directly.")
    );
    assert_eq!(answering.requests().await, Vec::<String>::new());
}

#[tokio::test]
async fn a_page_that_says_nothing_alerts_nobody() {
    let mut answering = paging().await;

    for text in ["page", "Page!", "page ?"] {
        assert_eq!(
            answering.answer(&in_sysadmin(ALICE, text)).await,
            Answer::only("What should I tell them? For example: `page lichess.org is down`."),
            "{text}"
        );
    }
    assert_eq!(answering.requests().await, Vec::<String>::new());
}

#[tokio::test]
async fn a_page_pagerduty_refuses_is_reported_and_not_announced() {
    let mut answering = Setup::default().start().await;
    Mock::given(method("POST"))
        .and(path(ENQUEUE))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "status": "invalid event",
            "message": "Event object is invalid",
            "errors": ["Routing key is invalid"],
        })))
        .mount(&answering.pagerduty)
        .await;

    assert_eq!(
        answering.answer(&page_by(ALICE)).await,
        Answer::only(
            ":warning: I couldn't page: pagerduty rejected the request (400 Bad Request): Event object is invalid: Routing key is invalid. Use PagerDuty directly."
        )
    );
}

#[tokio::test]
async fn soon_after_a_page_another_is_refused_saying_who_paged_and_how_long_ago() {
    let mut answering = paging().await;
    answering.answer(&page_by(ALICE)).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(90)).await;

    let answer = answering
        .answer(&in_sysadmin(BOB, "page the database is down too"))
        .await;

    assert_eq!(
        answer,
        Answer::only(
            ":hourglass: @_**|12** already paged 1m 30s ago. I won't page again for another 3m 30s. PagerDuty keeps escalating that alert until someone acknowledges it."
        )
    );
    assert_eq!(
        summaries(&answering).await,
        ["Alice Example: lichess.org is down"]
    );
}

#[tokio::test]
async fn a_page_is_refused_right_after_another() {
    let mut answering = paging().await;
    answering.answer(&page_by(ALICE)).await;
    tokio::time::pause();

    assert_eq!(
        answering.reply(&page_by(ALICE)).await,
        ":hourglass: @_**|12** already paged 0s ago. I won't page again for another 5m. PagerDuty keeps escalating that alert until someone acknowledges it."
    );
}

#[tokio::test]
async fn paging_works_again_once_the_cooldown_is_over_and_a_refused_page_does_not_extend_it() {
    let mut answering = paging_with_cooldown(Duration::from_mins(10)).await;
    answering.answer(&page_by(ALICE)).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_mins(9)).await;
    assert_eq!(
        answering.reply(&page_by(BOB)).await,
        ":hourglass: @_**|12** already paged 9m ago. I won't page again for another 1m. PagerDuty keeps escalating that alert until someone acknowledges it."
    );
    tokio::time::advance(Duration::from_mins(1)).await;
    tokio::time::resume();

    let answer = answering.answer(&page_by(BOB)).await;

    assert_eq!(answer.reply.markdown(), PAGED_BOB);
    assert_eq!(answering.requests().await, ["POST /v2/enqueue"; 2]);
}

#[tokio::test]
async fn a_page_that_failed_does_not_start_the_cooldown() {
    let mut answering = Setup::default().start().await;
    Mock::given(method("POST"))
        .and(path(ENQUEUE))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&answering.pagerduty)
        .await;
    answering.accepts_pages().await;
    assert_eq!(
        answering.reply(&page_by(ALICE)).await,
        ":warning: I couldn't page: pagerduty rejected the request (500 Internal Server Error): Internal Server Error. Use PagerDuty directly."
    );

    let answer = answering.answer(&page_by(BOB)).await;

    assert_eq!(answer.reply.markdown(), PAGED_BOB);
    assert_eq!(
        answer.announcement.unwrap().markdown(),
        ":rotating_light: @_**|13** paged on call:\n> lichess.org is down"
    );
}

#[tokio::test]
async fn a_cooldown_of_zero_never_refuses_a_page() {
    let mut answering = paging_with_cooldown(Duration::ZERO).await;

    for sender in [ALICE, ALICE, BOB] {
        let answer = answering.answer(&page_by(sender)).await;
        assert_eq!(answer.reply.markdown(), PAGED_BOB);
    }

    assert_eq!(answering.requests().await, ["POST /v2/enqueue"; 3]);
}

#[tokio::test]
async fn the_reply_names_who_is_alerted_first_and_counts_the_levels_behind_them() {
    let roster = only_policy(&[
        on_call_at(1, &[bob()]),
        on_call_at(2, &crowd(3)),
        on_call_at(3, &crowd(3)),
    ]);

    assert_eq!(
        reply_to_a_page_with(roster).await,
        ":rotating_light: Paged @_**|13**. Level 2 (3 people) and Level 3 (3 people) are backup."
    );
}

#[tokio::test]
async fn when_level_one_is_empty_the_first_level_with_anybody_is_alerted_and_the_reply_says_which()
{
    let roster = only_policy(&[
        on_call_at(1, &[]),
        on_call_at(2, &[alice(), bob(), carol()]),
        on_call_at(3, &crowd(3)),
    ]);

    assert_eq!(
        reply_to_a_page_with(roster).await,
        ":rotating_light: Paged @_**|12**, @_**|13** and Carol Example at level 2. Level 3 (3 people) is backup."
    );
}

#[tokio::test]
async fn more_than_three_people_alerted_first_are_counted_at_their_level_and_not_named() {
    let at_the_second = only_policy(&[
        on_call_at(1, &[]),
        on_call_at(2, &crowd(6)),
        on_call_at(3, &crowd(3)),
    ]);
    let at_the_first = only_policy(&[on_call_at(
        1,
        &[alice(), bob(), carol(), crowd(1).remove(0)],
    )]);

    assert_eq!(
        reply_to_a_page_with(at_the_second).await,
        ":rotating_light: Paged the 6 people at level 2. Level 3 (3 people) is backup."
    );
    assert_eq!(
        reply_to_a_page_with(at_the_first).await,
        ":rotating_light: Paged the 4 people at level 1."
    );
}

#[tokio::test]
async fn with_nobody_behind_them_the_reply_ends_after_who_is_alerted() {
    let alone = only_policy(&[on_call_at(1, &[bob()])]);
    let two = only_policy(&[on_call_at(1, &[alice(), bob()]), on_call_at(2, &[])]);

    assert_eq!(
        reply_to_a_page_with(alone).await,
        ":rotating_light: Paged @_**|13**."
    );
    assert_eq!(
        reply_to_a_page_with(two).await,
        ":rotating_light: Paged @_**|12** and @_**|13**."
    );
}

#[tokio::test]
async fn a_level_behind_with_nobody_on_call_is_left_out_of_the_backup() {
    let roster = only_policy(&[
        on_call_at(1, &[bob()]),
        on_call_at(2, &[]),
        on_call_at(3, &[carol()]),
        on_call_at(4, &crowd(2)),
        on_call_at(5, &[alice()]),
    ]);

    assert_eq!(
        reply_to_a_page_with(roster).await,
        ":rotating_light: Paged @_**|13**. Level 3 (1 person), Level 4 (2 people) and Level 5 (1 person) are backup."
    );
}

#[tokio::test]
async fn a_page_with_nobody_on_call_at_any_level_is_sent_and_the_reply_warns() {
    let roster = only_policy(&[on_call_at(1, &[]), on_call_at(2, &[])]);

    assert_eq!(
        reply_to_a_page_with(roster).await,
        ":rotating_light: Paged, but nobody is on call at any level. Use PagerDuty directly."
    );
}

#[tokio::test]
async fn with_several_policies_or_none_the_reply_does_not_say_who_is_alerted() {
    let mut several = fishnet_and_lichess();
    for policy in &mut several.policies {
        policy.levels = vec![on_call_at(1, &[bob()]), on_call_at(2, &[carol()])];
    }
    let none = Roster { policies: vec![] };

    assert_eq!(reply_to_a_page_with(several).await, PAGED_WHOEVER);
    assert_eq!(reply_to_a_page_with(none).await, PAGED_WHOEVER);
}

#[tokio::test]
async fn a_name_from_pagerduty_that_holds_a_mention_is_named_without_notifying() {
    let roster = only_policy(&[on_call_at(
        1,
        &[
            user("PEVERY1", "@**all**"),
            user("PGROUP1", "x @*sysadmins*"),
        ],
    )]);

    assert_eq!(
        reply_to_a_page_with(roster).await,
        ":rotating_light: Paged @_**all** and x @_*sysadmins*."
    );
}
