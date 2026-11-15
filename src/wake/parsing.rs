use std::time::Duration;

use nutype::nutype;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Page(Option<Trouble>),
    Wake(Option<Duration>),
    Stop,
    Reset,
    Who,
    Help,
    Unrecognized,
}

#[nutype(
    sanitize(trim),
    validate(not_empty),
    derive(Debug, Clone, PartialEq, Eq, Deref, Display)
)]
pub struct Trouble(String);

#[derive(Clone, Copy)]
enum Piece {
    Amount(u64),
    Seconds(u64),
    Other,
}

const CLOSING_PUNCTUATION: [char; 3] = ['.', '!', '?'];
const QUOTING: [char; 10] = ['`', '"', '\'', '“', '”', '„', '‘', '’', '«', '»'];
const PAGE: &str = "page";
const WAKE: [&str; 2] = ["wake", "me"];
const STOP: &str = "stop";
const RESET: &str = "reset";
const BACK_TO: [&str; 2] = ["back", "to"];
const DEFAULTS: [&str; 2] = ["default", "defaults"];
const WHO: &str = "who";
const ASKING_WHO: [&str; 3] = ["on call", "oncall", "status"];
const HELP: &str = "help";

const ONE: [&str; 2] = ["a", "an"];
const UNITS: [(&[&str], u64); 3] = [
    (&["m", "min", "mins", "minute", "minutes"], 60),
    (&["h", "hr", "hrs", "hour", "hours"], 60 * 60),
    (&["d", "day", "days"], 24 * 60 * 60),
];

pub fn parse(text: &str) -> Command {
    let lowered = text.to_lowercase();
    let words: Vec<&str> = lowered
        .trim()
        .trim_end_matches(CLOSING_PUNCTUATION)
        .split_whitespace()
        .map(bare)
        .collect();
    let wording = words.join(" ");
    match words.as_slice() {
        [first, ..] if first.starts_with(QUOTING) => Command::Unrecognized,
        [first, ..] if *first == PAGE => Command::Page(trouble(text)),
        [wake, me, rest @ ..] if [*wake, *me] == WAKE => Command::Wake(duration(rest)),
        [first, ..] if *first == STOP => Command::Stop,
        [first, ..] if *first == RESET => Command::Reset,
        [back, to, ..] if [*back, *to] == BACK_TO => Command::Reset,
        [first, ..] if first.starts_with(WHO) => Command::Who,
        _ if ASKING_WHO.contains(&wording.as_str()) => Command::Who,
        _ if wording == HELP => Command::Help,
        _ if words.iter().any(|word| DEFAULTS.contains(word)) => Command::Reset,
        _ => Command::Unrecognized,
    }
}

fn bare(word: &str) -> &str {
    let named = word.trim_start_matches(is_punctuation);
    let quoted = word
        .strip_suffix(named)
        .is_some_and(|before| before.contains(QUOTING));
    if quoted {
        word
    } else {
        named.trim_end_matches(is_punctuation)
    }
}

fn is_punctuation(character: char) -> bool {
    !character.is_alphanumeric()
}

fn trouble(text: &str) -> Option<Trouble> {
    let (_, said) = text.trim().split_once(char::is_whitespace)?;
    let says_something = said
        .chars()
        .any(|character| !character.is_whitespace() && !CLOSING_PUNCTUATION.contains(&character));
    says_something
        .then(|| Trouble::try_new(said).ok())
        .flatten()
}

fn duration(words: &[&str]) -> Option<Duration> {
    let mut pieces = words.iter().flat_map(|word| pieces(word)).peekable();
    let mut seconds: Option<u64> = None;
    while let Some(piece) = pieces.next() {
        if let (Piece::Amount(amount), Some(Piece::Seconds(unit))) = (piece, pieces.peek()) {
            let already = seconds.unwrap_or_default();
            seconds = Some(already.saturating_add(amount.saturating_mul(*unit)));
            pieces.next();
        }
    }
    seconds.map(Duration::from_secs)
}

fn pieces(word: &str) -> Vec<Piece> {
    if !word.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return vec![Piece::Other];
    }
    word.as_bytes()
        .chunk_by(|one, next| one.is_ascii_digit() == next.is_ascii_digit())
        .map(|run| std::str::from_utf8(run).map_or(Piece::Other, piece))
        .collect()
}

fn piece(run: &str) -> Piece {
    if run.starts_with(|first: char| first.is_ascii_digit()) {
        return Piece::Amount(run.parse().unwrap_or(u64::MAX));
    }
    if ONE.contains(&run) {
        return Piece::Amount(1);
    }
    UNITS
        .iter()
        .find_map(|(names, seconds)| names.contains(&run).then_some(Piece::Seconds(*seconds)))
        .unwrap_or(Piece::Other)
}

#[cfg(test)]
mod tests;
