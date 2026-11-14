use std::collections::BTreeMap;

use super::links::Links;
use super::members::ChatMember;
use super::name::Name;
use crate::chat::ChatUserId;
use crate::pagerduty::{User, UserId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    users: BTreeMap<UserId, User>,
    chat_users: BTreeMap<UserId, ChatUserId>,
    pagerduty_users: BTreeMap<ChatUserId, UserId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MatchedBy {
    Link,
    Email,
    FullName,
}

#[derive(Clone, Copy, Debug)]
struct Match<'a> {
    user: &'a User,
    chat_user: ChatUserId,
    by: MatchedBy,
}

impl Directory {
    /// ```
    /// use wokesy::chat::ChatUserId;
    /// use wokesy::pagerduty::{User, UserId};
    /// use wokesy::people::{ChatMember, Directory, Email, Links, Name};
    ///
    /// let alice = User {
    ///     id: UserId::try_new("PALICE1").unwrap(),
    ///     name: Name::try_new("Alice Example").unwrap(),
    ///     email: Email::try_new("alice@example.com").unwrap(),
    /// };
    /// let bob = User {
    ///     id: UserId::try_new("PBOB001").unwrap(),
    ///     name: Name::try_new("Bob Example").unwrap(),
    ///     email: Email::try_new("bob@pagerduty.example.com").unwrap(),
    /// };
    /// let members = [ChatMember {
    ///     id: ChatUserId(12),
    ///     name: Name::try_new("Alice in Chat").unwrap(),
    ///     emails: vec![Email::try_new("alice@example.com").unwrap()],
    /// }];
    /// let links: Links = "bob@pagerduty.example.com=34".parse().unwrap();
    ///
    /// let directory = Directory::matching(&[alice.clone(), bob.clone()], &members, &links);
    ///
    /// assert_eq!(directory.chat_user_of(&alice.id), Some(ChatUserId(12)));
    /// assert_eq!(directory.chat_user_of(&bob.id), Some(ChatUserId(34)));
    /// assert_eq!(directory.pagerduty_user_of(ChatUserId(34)), Some(&bob));
    /// assert_eq!(directory.pagerduty_user_of(ChatUserId(56)), None);
    /// assert_eq!(directory.name_of(&alice.id), Some(&alice.name));
    /// ```
    #[must_use]
    pub fn matching(users: &[User], members: &[ChatMember], links: &Links) -> Self {
        let mut matches: Vec<Match<'_>> = users
            .iter()
            .filter_map(|user| matched(user, members, links))
            .collect();
        matches.sort_by_key(|matched| (matched.chat_user, matched.by));
        Self {
            users: users
                .iter()
                .map(|user| (user.id.clone(), user.clone()))
                .collect(),
            chat_users: matches
                .iter()
                .map(|matched| (matched.user.id.clone(), matched.chat_user))
                .collect(),
            pagerduty_users: matches
                .chunk_by(|one, other| one.chat_user == other.chat_user)
                .filter_map(most_surely_matched)
                .map(|matched| (matched.chat_user, matched.user.id.clone()))
                .collect(),
        }
    }

    #[must_use]
    pub fn chat_user_of(&self, user: &UserId) -> Option<ChatUserId> {
        self.chat_users.get(user).copied()
    }

    #[must_use]
    pub fn pagerduty_user_of(&self, chat: ChatUserId) -> Option<&User> {
        self.users.get(self.pagerduty_users.get(&chat)?)
    }

    #[must_use]
    pub fn name_of(&self, user: &UserId) -> Option<&Name> {
        self.users.get(user).map(|user| &user.name)
    }
}

fn matched<'a>(user: &'a User, members: &[ChatMember], links: &Links) -> Option<Match<'a>> {
    let by = |by, chat_user| Match {
        user,
        chat_user,
        by,
    };
    links
        .chat_user_of(&user.email)
        .map(|linked| by(MatchedBy::Link, linked))
        .or_else(|| {
            only(members.iter().filter(|member| has_email_of(member, user)))
                .map(|member| by(MatchedBy::Email, member.id))
        })
        .or_else(|| {
            only(members.iter().filter(|member| has_name_of(member, user)))
                .map(|member| by(MatchedBy::FullName, member.id))
        })
}

fn has_email_of(member: &ChatMember, user: &User) -> bool {
    member.emails.contains(&user.email)
}

fn has_name_of(member: &ChatMember, user: &User) -> bool {
    member.name.to_lowercase() == user.name.to_lowercase()
}

fn only<T>(mut candidates: impl Iterator<Item = T>) -> Option<T> {
    let first = candidates.next()?;
    candidates.next().is_none().then_some(first)
}

fn most_surely_matched<'a, 'b>(sharing_a_chat_user: &'b [Match<'a>]) -> Option<&'b Match<'a>> {
    let surest = sharing_a_chat_user.first()?;
    let contested = sharing_a_chat_user
        .get(1)
        .is_some_and(|next| next.by == surest.by);
    (!contested).then_some(surest)
}

#[cfg(test)]
mod tests;
