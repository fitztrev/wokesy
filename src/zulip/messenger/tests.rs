use std::collections::BTreeMap;

use secrecy::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::chat::{ChannelName, ChatUserId, TopicName};
use crate::zulip::{ApiError, BotEmail, InboxConfig};

struct FakeZulip {
    server: MockServer,
    messenger: ZulipMessenger,
}

impl FakeZulip {
    async fn replying(reply: Value) -> Self {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&reply))
            .mount(&server)
            .await;
        let config = Config {
            site: server.uri().parse().unwrap(),
            email: BotEmail::try_new("wokesy-bot@zulip.example.com").unwrap(),
            api_key: SecretString::from("key"),
            inbox: InboxConfig::default(),
        };
        Self {
            server,
            messenger: ZulipMessenger::new(reqwest::Client::new(), &config),
        }
    }

    async fn accepting() -> Self {
        Self::replying(json!({ "result": "success", "msg": "", "id": 42 })).await
    }

    async fn only_form(&self) -> BTreeMap<String, String> {
        let mut requests = self.server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        url::form_urlencoded::parse(&requests.remove(0).body)
            .into_owned()
            .collect()
    }
}

fn message() -> OutgoingMessage {
    OutgoingMessage::new(":pager: **On call now:** @_**|12**".to_owned())
}

fn form(fields: &[(&str, &str)]) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[tokio::test]
async fn a_direct_message_is_sent_as_type_direct_to_a_bracketed_user_id_list() {
    let zulip = FakeZulip::accepting().await;
    let conversation = Conversation::Direct {
        user_id: ChatUserId(17),
    };

    zulip
        .messenger
        .send(&conversation, &message())
        .await
        .unwrap();

    assert_eq!(
        zulip.only_form().await,
        form(&[
            ("type", "direct"),
            ("to", "[17]"),
            ("content", message().markdown()),
        ])
    );
}

#[tokio::test]
async fn a_channel_message_is_sent_as_type_channel_to_the_channel_and_topic() {
    let zulip = FakeZulip::accepting().await;
    let conversation = Conversation::Topic {
        channel: ChannelName::try_new("sysadmin").unwrap(),
        topic: TopicName::try_new("on call").unwrap(),
    };

    zulip
        .messenger
        .send(&conversation, &message())
        .await
        .unwrap();

    assert_eq!(
        zulip.only_form().await,
        form(&[
            ("type", "channel"),
            ("to", "sysadmin"),
            ("topic", "on call"),
            ("content", message().markdown()),
        ])
    );
}

#[tokio::test]
async fn a_non_success_result_from_zulip_is_a_failed_delivery_carrying_its_error_message() {
    let zulip = FakeZulip::replying(json!({
        "result": "error",
        "msg": "Channel 'sysadmin' does not exist",
        "code": "STREAM_DOES_NOT_EXIST",
    }))
    .await;
    let conversation = Conversation::Topic {
        channel: ChannelName::try_new("sysadmin").unwrap(),
        topic: TopicName::try_new("on call").unwrap(),
    };

    let error = zulip
        .messenger
        .send(&conversation, &message())
        .await
        .unwrap_err();

    assert_eq!(error.conversation, conversation);
    let rejection = error.cause.downcast_ref::<ApiError>();
    assert!(matches!(
        rejection,
        Some(ApiError::Rejected { reason, .. }) if reason == "Channel 'sysadmin' does not exist"
    ));
}
