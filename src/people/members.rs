use super::email::Email;
use super::name::Name;
use crate::chat::ChatUserId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMember {
    pub id: ChatUserId,
    pub name: Name,
    pub emails: Vec<Email>,
}

#[derive(Debug, thiserror::Error)]
#[error("could not read the chat members")]
pub struct MembersError(#[source] pub Box<dyn std::error::Error + Send + Sync>);

pub trait ChatMembers: Send + Sync + 'static {
    fn members(&self) -> impl Future<Output = Result<Vec<ChatMember>, MembersError>> + Send;
}
