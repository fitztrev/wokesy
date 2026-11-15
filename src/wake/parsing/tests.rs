use super::*;

const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);

fn page(trouble: &str) -> Command {
    Command::Page(Some(Trouble::try_new(trouble).unwrap()))
}

fn wake(lasting: Duration) -> Command {
    Command::Wake(Some(lasting))
}

#[test]
fn wake_me_takes_its_duration_from_an_amount_and_a_unit() {
    for (text, lasting) in [
        ("wake me for 4 hours", 4 * HOUR),
        ("wake me for 4h", 4 * HOUR),
        ("wake me for 90 minutes", 90 * MINUTE),
        ("wake me for 1h30m", 90 * MINUTE),
        ("wake me for 1 hour 30 minutes", 90 * MINUTE),
        ("wake me for an hour", HOUR),
        ("wake me for a day", DAY),
        ("wake me for anything in the next 4 hours", 4 * HOUR),
        ("wake me 2 days", 2 * DAY),
        (
            "wake me for 1 day, 2 hours and 3 minutes",
            DAY + 2 * HOUR + 3 * MINUTE,
        ),
        ("wake me for 1d2h3m", DAY + 2 * HOUR + 3 * MINUTE),
    ] {
        assert_eq!(parse(text), wake(lasting), "{text}");
    }
}

#[test]
fn every_spelling_of_a_unit_is_understood() {
    for (units, one) in [
        (["m", "min", "mins", "minute", "minutes"].as_slice(), MINUTE),
        (["h", "hr", "hrs", "hour", "hours"].as_slice(), HOUR),
        (["d", "day", "days"].as_slice(), DAY),
    ] {
        for unit in units {
            assert_eq!(
                parse(&format!("wake me for 3 {unit}")),
                wake(3 * one),
                "{unit}"
            );
            assert_eq!(
                parse(&format!("wake me for 3{unit}")),
                wake(3 * one),
                "{unit}"
            );
        }
    }
}

#[test]
fn wake_me_without_an_amount_and_a_unit_has_no_duration() {
    for text in [
        "wake me",
        "wake me for anything",
        "wake me for 4",
        "wake me for hours",
        "wake me at 3 pm",
        "wake me for 1.5 hours",
    ] {
        assert_eq!(parse(text), Command::Wake(None), "{text}");
    }
}

#[test]
fn wake_me_for_nothing_is_a_duration_of_zero() {
    assert_eq!(parse("wake me for 0 hours"), wake(Duration::ZERO));
}

#[test]
fn an_amount_too_large_to_count_is_the_longest_duration_there_is() {
    assert_eq!(
        parse("wake me for 99999999999999999999999 days"),
        wake(Duration::from_secs(u64::MAX))
    );
    assert_eq!(
        parse("wake me for 18446744073709551615 hours and 5 days"),
        wake(Duration::from_secs(u64::MAX))
    );
}

#[test]
fn commands_match_whatever_their_case_spacing_and_closing_punctuation() {
    for (text, command) in [
        ("  Wake  Me for 4 Hours. ", wake(4 * HOUR)),
        ("WAKE ME FOR 4H!", wake(4 * HOUR)),
        ("wake me\nfor 4 hours", wake(4 * HOUR)),
        ("Stop waking me.", Command::Stop),
        ("Back To Defaults!", Command::Reset),
        ("Who is on call?", Command::Who),
        ("On Call?", Command::Who),
        ("HELP!!", Command::Help),
        (" help ", Command::Help),
    ] {
        assert_eq!(parse(text), command, "{text}");
    }
}

#[test]
fn page_keeps_the_case_and_punctuation_of_what_is_wrong() {
    for (text, trouble) in [
        ("page lichess.org is down", "lichess.org is down"),
        ("Page Lichess.org is DOWN!", "Lichess.org is DOWN!"),
        ("PAGE   is the DB ok?  ", "is the DB ok?"),
        ("page: lila won't start", "lila won't start"),
        (
            "page the database is down\nreplication stopped at 03:12\n\nPlease look.",
            "the database is down\nreplication stopped at 03:12\n\nPlease look.",
        ),
    ] {
        assert_eq!(parse(text), page(trouble), "{text}");
    }
}

#[test]
fn page_with_nothing_said_after_it_has_nothing_to_tell() {
    for text in ["page", "Page", "page!", "page ?", " page  . "] {
        assert_eq!(parse(text), Command::Page(None), "{text}");
    }
}

#[test]
fn page_is_only_the_whole_word() {
    for text in ["pager is beeping", "pages are down", "paging on call"] {
        assert_eq!(parse(text), Command::Unrecognized, "{text}");
    }
}

#[test]
fn punctuation_after_a_command_word_is_ignored() {
    for (text, command) in [
        ("page: db down", page("db down")),
        ("stop, please", Command::Stop),
        ("reset!! now", Command::Reset),
        ("back to: defaults", Command::Reset),
        ("who? me?", Command::Who),
        ("wake me, for 4h", wake(4 * HOUR)),
    ] {
        assert_eq!(parse(text), command, "{text}");
    }
}

#[test]
fn a_first_word_in_backticks_or_quotes_is_talk_about_a_command() {
    for text in [
        "`page` is how you alert on call",
        "`page db is down`",
        "```\npage db is down\n```",
        "\"page\" alerts whoever is on call",
        "'stop' ends it early",
        "“wake me for 4 hours” is what I typed",
        "‘who’ shows the roster",
        "«reset»",
        "(`page`) db is down",
        "`help`",
        "\"back to defaults\"",
        "`stop` goes back to the default",
    ] {
        assert_eq!(parse(text), Command::Unrecognized, "{text}");
    }
}

#[test]
fn a_command_word_right_after_a_backtick_or_a_quote_is_not_the_command() {
    for text in [
        "the `defaults` are fine",
        "what does \"default\" mean",
        "is ‘defaults’ a command",
        "wake `me` for 4h",
        "on `call`",
    ] {
        assert_eq!(parse(text), Command::Unrecognized, "{text}");
    }
    assert_eq!(parse("wake me for `4h`"), Command::Wake(None));
    assert_eq!(parse("page `lila` won't start"), page("`lila` won't start"));
}

#[test]
fn stop_is_any_text_starting_with_stop() {
    for text in ["stop waking me", "stop", "stop it", "stop, please"] {
        assert_eq!(parse(text), Command::Stop, "{text}");
    }
}

#[test]
fn reset_is_back_to_or_reset_at_the_start_or_the_word_default_anywhere() {
    for text in [
        "back to defaults",
        "reset",
        "defaults",
        "reset everything",
        "back to the schedule",
        "go back to the defaults, please",
        "restore the default",
    ] {
        assert_eq!(parse(text), Command::Reset, "{text}");
    }
}

#[test]
fn who_is_asked_in_several_ways() {
    for text in [
        "who is on call",
        "who",
        "on call",
        "oncall",
        "status",
        "who's up",
        "who is first in line tonight",
    ] {
        assert_eq!(parse(text), Command::Who, "{text}");
    }
}

#[test]
fn a_command_named_at_the_start_wins_over_the_word_default_later_on() {
    assert_eq!(
        parse("page the default route is down"),
        page("the default route is down")
    );
    assert_eq!(parse("wake me for 2h by default"), wake(2 * HOUR));
    assert_eq!(parse("stop the defaults"), Command::Stop);
    assert_eq!(parse("who is on call by default"), Command::Who);
}

#[test]
fn anything_else_is_unrecognized() {
    for text in [
        "",
        "   ",
        "hello",
        "thanks!",
        "help me",
        "wake up",
        "wake",
        "back tomorrow",
        "resetting",
        "on call tonight",
        "the status",
        "please page someone",
    ] {
        assert_eq!(parse(text), Command::Unrecognized, "{text}");
    }
}
