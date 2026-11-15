use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::*;
use crate::wake::Config;
use crate::wake::testing::{ALICE, Answering, BOB, ENQUEUE, STRANGER, Setup, in_sysadmin};

const PAGED_AND_ON_CALL_NOW: &str = "\
:rotating_light: Paged. PagerDuty is alerting whoever is on call.

:pager: **On call now:** @_**|13**

```spoiler schedule
**Lichess · Level 1**
- @_**|13**

**Lichess · Level 2**
- Carol Example
```";

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

    assert_eq!(answer.reply.markdown(), PAGED_AND_ON_CALL_NOW);
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
    assert_eq!(answer.reply.markdown(), PAGED_AND_ON_CALL_NOW);
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

        assert_eq!(
            answer.reply.markdown(),
            ":rotating_light: Paged. PagerDuty is alerting whoever is on call."
        );
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

    assert_eq!(answer.reply.markdown(), PAGED_AND_ON_CALL_NOW);
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

    assert_eq!(answer.reply.markdown(), PAGED_AND_ON_CALL_NOW);
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
        assert_eq!(answer.reply.markdown(), PAGED_AND_ON_CALL_NOW);
    }

    assert_eq!(answering.requests().await, ["POST /v2/enqueue"; 3]);
}
