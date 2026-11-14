use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Deserialize;

use super::email::Email;
use crate::chat::ChatUserId;

const BETWEEN_LINKS: char = ',';
const BETWEEN_EMAIL_AND_USER_ID: char = '=';

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Links(BTreeMap<Email, ChatUserId>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("link {position} is not an `email=zulip_user_id` pair")]
pub struct MalformedLink {
    pub position: usize,
}

impl Links {
    /// ```
    /// use wokesy::chat::ChatUserId;
    /// use wokesy::people::{Email, Links};
    ///
    /// let links: Links = "alice@example.com=12, bob@example.com=34".parse().unwrap();
    /// let alice = Email::try_new("alice@example.com").unwrap();
    /// let carol = Email::try_new("carol@example.com").unwrap();
    /// assert_eq!(links.chat_user_of(&alice), Some(ChatUserId(12)));
    /// assert_eq!(links.chat_user_of(&carol), None);
    /// ```
    #[must_use]
    pub fn chat_user_of(&self, email: &Email) -> Option<ChatUserId> {
        self.0.get(email).copied()
    }
}

impl FromStr for Links {
    type Err = MalformedLink;

    fn from_str(links: &str) -> Result<Self, Self::Err> {
        links
            .split(BETWEEN_LINKS)
            .enumerate()
            .filter(|(_, link)| !link.trim().is_empty())
            .map(|(index, link)| {
                email_and_user_id(link).ok_or(MalformedLink {
                    position: index + 1,
                })
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

impl TryFrom<String> for Links {
    type Error = MalformedLink;

    fn try_from(links: String) -> Result<Self, Self::Error> {
        links.parse()
    }
}

fn email_and_user_id(link: &str) -> Option<(Email, ChatUserId)> {
    let (email, user_id) = link.rsplit_once(BETWEEN_EMAIL_AND_USER_ID)?;
    let email = Email::try_new(email).ok()?;
    let user_id = user_id.trim().parse().ok()?;
    Some((email, ChatUserId(user_id)))
}

#[cfg(test)]
mod tests;
