use crate::people::{ChatMember, ChatMembers, Email, MembersError, Name};

use super::Config;
use super::api::{Api, Member};

#[derive(Clone, Debug)]
pub struct ZulipMembers {
    api: Api,
}

impl ZulipMembers {
    #[must_use]
    pub fn new(http: reqwest::Client, config: &Config) -> Self {
        Self {
            api: Api::new(http, config),
        }
    }
}

impl ChatMembers for ZulipMembers {
    async fn members(&self) -> Result<Vec<ChatMember>, MembersError> {
        let members = self
            .api
            .members()
            .await
            .map_err(|error| MembersError(Box::new(error)))?;
        Ok(members
            .into_iter()
            .filter(|member| member.is_active && !member.is_bot)
            .filter_map(chat_member)
            .collect())
    }
}

fn chat_member(member: Member) -> Option<ChatMember> {
    let id = member.id;
    let name = match Name::try_new(member.full_name) {
        Ok(name) => name,
        Err(error) => {
            tracing::warn!(user_id = id.0, error = %error, "zulip member with an unusable name skipped");
            return None;
        }
    };
    let mut emails = Vec::new();
    for address in member.delivery_email.into_iter().chain([member.email]) {
        match Email::try_new(address) {
            Ok(email) if emails.contains(&email) => {}
            Ok(email) => emails.push(email),
            Err(error) => {
                tracing::warn!(user_id = id.0, error = %error, "zulip member with an unusable email skipped");
                return None;
            }
        }
    }
    Some(ChatMember { id, name, emails })
}

#[cfg(test)]
mod tests;
