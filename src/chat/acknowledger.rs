use super::message::MessageId;

#[derive(Debug, thiserror::Error)]
#[error("failed to acknowledge message {}", .message.0)]
pub struct AcknowledgementError {
    pub message: MessageId,
    #[source]
    pub cause: Box<dyn std::error::Error + Send + Sync>,
}

pub trait Acknowledger: Send + Sync + 'static {
    fn acknowledge(
        &self,
        message: MessageId,
    ) -> impl Future<Output = Result<(), AcknowledgementError>> + Send;
}
