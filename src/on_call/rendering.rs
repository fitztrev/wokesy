use super::changes::Change;
use super::roster::{Roster, Shift};
use crate::chat::{ChatUserId, silenced, time_tag};
use crate::pagerduty::{EscalationLevel, EscalationPolicyName, ScheduleName, UserRef};
use crate::people::{Directory, Name};

const ON_CALL_NOW: &str = ":pager: **On call now**";
const ON_CALL_CHANGED: &str = ":pager: **On call changed**";
const NO_POLICY_IN_USE: &str = "No escalation policy is used by a service, so nobody is on call.";
const NOBODY_ON_CALL: &str = ":warning: nobody";
const NOBODY: &str = "nobody";
const BETWEEN_PEOPLE: &str = ", ";

/// ```
/// use wokesy::on_call::policy_in_chat;
/// use wokesy::pagerduty::EscalationPolicyName;
///
/// let name = |name: &str| EscalationPolicyName::try_new(name).unwrap();
///
/// assert_eq!(policy_in_chat(&name("Lichess")), "**Lichess**");
/// assert_eq!(policy_in_chat(&name("Ops @**all**")), "**Ops @_**all****");
/// ```
#[must_use]
pub fn policy_in_chat(name: &EscalationPolicyName) -> String {
    bold(name)
}

/// ```
/// use wokesy::on_call::schedule_in_chat;
/// use wokesy::pagerduty::ScheduleName;
///
/// let name = |name: &str| ScheduleName::try_new(name).unwrap();
///
/// assert_eq!(schedule_in_chat(&name("Primary rota")), "**Primary rota**");
/// assert_eq!(schedule_in_chat(&name("Rota of @*sysadmins*")), "**Rota of @_*sysadmins***");
/// ```
#[must_use]
pub fn schedule_in_chat(name: &ScheduleName) -> String {
    bold(name)
}

/// ```
/// use wokesy::on_call::name_in_chat;
/// use wokesy::people::Name;
///
/// let name = |name: &str| Name::try_new(name).unwrap();
///
/// assert_eq!(name_in_chat(&name("Carol Example")), "Carol Example");
/// assert_eq!(name_in_chat(&name("@**all**")), "@_**all**");
/// ```
#[must_use]
pub fn name_in_chat(name: &Name) -> String {
    silenced(name)
}

fn bold(name: &str) -> String {
    format!("**{}**", silenced(name))
}

/// ```
/// use wokesy::on_call::{Roster, on_call_now};
/// use wokesy::people::{Directory, Links};
///
/// let nobody_to_call = Roster::new(vec![], vec![]);
/// let directory = Directory::matching(&[], &[], &Links::default());
///
/// assert_eq!(
///     on_call_now(&nobody_to_call, &directory),
///     ":pager: **On call now**\nNo escalation policy is used by a service, so nobody is on call."
/// );
/// ```
#[must_use]
pub fn on_call_now(roster: &Roster, directory: &Directory) -> String {
    if roster.policies.is_empty() {
        return under(ON_CALL_NOW, [NO_POLICY_IN_USE.to_owned()]);
    }
    let levels = roster.policies.iter().flat_map(|policy| {
        policy.levels.iter().map(|level| {
            line(
                &policy.name,
                level.number,
                &on_call(&level.shifts, directory),
            )
        })
    });
    under(ON_CALL_NOW, levels)
}

pub fn on_call_changed(changes: &[Change<'_>], directory: &Directory) -> String {
    let levels = changes.iter().map(|change| {
        let now = on_call(change.on_call, directory);
        let was = previously_on_call(change.was_on_call, directory);
        line(change.policy, change.level, &format!("{now} (was {was})"))
    });
    under(ON_CALL_CHANGED, levels)
}

fn under(heading: &str, lines: impl IntoIterator<Item = String>) -> String {
    std::iter::once(heading.to_owned())
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn line(policy: &EscalationPolicyName, level: EscalationLevel, on_call: &str) -> String {
    format!("- {}, level {level}: {on_call}", policy_in_chat(policy))
}

fn on_call(shifts: &[Shift], directory: &Directory) -> String {
    if shifts.is_empty() {
        return NOBODY_ON_CALL.to_owned();
    }
    listed(shifts.iter().map(|shift| {
        let person = person(&shift.user, directory);
        match shift.ends {
            Some(ends) => format!("{person} until {}", time_tag(ends)),
            None => person,
        }
    }))
}

fn previously_on_call(shifts: &[Shift], directory: &Directory) -> String {
    if shifts.is_empty() {
        return NOBODY.to_owned();
    }
    listed(shifts.iter().map(|shift| person(&shift.user, directory)))
}

fn listed(people: impl Iterator<Item = String>) -> String {
    people.collect::<Vec<_>>().join(BETWEEN_PEOPLE)
}

fn person(user: &UserRef, directory: &Directory) -> String {
    directory
        .chat_user_of(&user.id)
        .map_or_else(|| name_in_chat(&user.name), ChatUserId::silent_mention)
}

#[cfg(test)]
mod tests;
