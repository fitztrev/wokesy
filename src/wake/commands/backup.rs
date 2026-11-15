use crate::on_call::Level;

const BETWEEN: &str = ", ";
const BEFORE_THE_LAST: &str = " and ";

pub(super) fn backup<'a>(behind: impl IntoIterator<Item = &'a Level>) -> Option<String> {
    let levels: Vec<String> = behind
        .into_iter()
        .filter(|level| !level.shifts.is_empty())
        .map(|level| format!("Level {} ({})", level.number, people(level.shifts.len())))
        .collect();
    let are = match levels.len() {
        0 => return None,
        1 => "is",
        _ => "are",
    };
    Some(format!("{} {are} backup.", listed(&levels)))
}

pub(super) fn people(count: usize) -> String {
    if count == 1 {
        "1 person".to_owned()
    } else {
        format!("{count} people")
    }
}

pub(super) fn listed(all: &[String]) -> String {
    match all {
        [several @ .., last] if !several.is_empty() => {
            format!("{}{BEFORE_THE_LAST}{last}", several.join(BETWEEN))
        }
        _ => all.concat(),
    }
}
