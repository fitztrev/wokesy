use crate::chat::{Conversation, DeliveryError, Messenger, OutgoingMessage};

use super::Config;
use super::api::Api;

#[derive(Clone, Debug)]
pub struct ZulipMessenger {
    api: Api,
}

impl ZulipMessenger {
    #[must_use]
    pub fn new(http: reqwest::Client, config: &Config) -> Self {
        Self {
            api: Api::new(http, config),
        }
    }
}

impl Messenger for ZulipMessenger {
    async fn send(
        &self,
        conversation: &Conversation,
        message: &OutgoingMessage,
    ) -> Result<(), DeliveryError> {
        self.api
            .send_message(conversation, message)
            .await
            .map_err(|error| DeliveryError {
                conversation: conversation.clone(),
                cause: Box::new(error),
            })
    }
}

#[cfg(test)]
mod tests;
