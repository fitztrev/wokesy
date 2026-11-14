use nutype::nutype;

use super::conversation::{ChatUserId, Conversation};
use super::message::MessageId;
use crate::people::Name;

/// ```
/// use wokesy::chat::CommandText;
///
/// assert_eq!(CommandText::new("  wake me for 4 hours \n").as_str(), "wake me for 4 hours");
/// ```
#[nutype(sanitize(trim), derive(Debug, Clone, PartialEq, Eq, Deref))]
pub struct CommandText(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingMessage {
    pub id: MessageId,
    pub sender: ChatUserId,
    pub sender_name: Name,
    pub reply_to: Conversation,
    pub text: CommandText,
}
