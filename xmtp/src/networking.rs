use std::collections::HashMap;

use async_trait::async_trait;
use xmtp_networking::grpc_api_helper::Subscription;
use xmtp_proto::xmtp::message_api::v1::{Envelope, PagingInfo, PublishResponse, QueryResponse};

#[async_trait]
pub trait XmtpApiClient {
    async fn publish(
        &mut self,
        token: String,
        envelopes: Vec<Envelope>,
        // TODO: use error enums
    ) -> Result<PublishResponse, String>;

    async fn query(
        &self,
        topic: String,
        start_time: Option<u64>,
        end_time: Option<u64>,
        paging_info: Option<PagingInfo>,
        // TODO: use error enums
    ) -> Result<QueryResponse, String>;

    // TODO: use error enums
    async fn subscribe(&mut self, topics: Vec<String>) -> Result<Subscription, String>;
}

pub struct MockXmtpApiClient {
    messages: HashMap<String, Vec<Envelope>>,
}

impl MockXmtpApiClient {
    pub fn new() -> Self {
        Self {
            messages: HashMap::new(),
        }
    }
}

impl Default for MockXmtpApiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl XmtpApiClient for MockXmtpApiClient {
    async fn publish(
        &mut self,
        _token: String,
        envelopes: Vec<Envelope>,
    ) -> Result<PublishResponse, String> {
        for envelope in envelopes {
            let topic = envelope.content_topic.clone();
            let mut existing: Vec<Envelope> = match self.messages.get(&topic) {
                Some(existing_envelopes) => existing_envelopes.clone(),
                None => vec![],
            };
            existing.push(envelope);
            self.messages.insert(topic, existing);
        }

        Ok(PublishResponse {})
    }

    async fn query(
        &self,
        topic: String,
        _start_time: Option<u64>,
        _end_time: Option<u64>,
        _paging_info: Option<PagingInfo>,
    ) -> Result<QueryResponse, String> {
        let envelopes: Vec<Envelope> = match self.messages.get(&topic) {
            Some(envelopes) => envelopes.clone(),
            None => vec![],
        };

        Ok(QueryResponse {
            envelopes,
            paging_info: None,
        })
    }

    async fn subscribe(&mut self, _topics: Vec<String>) -> Result<Subscription, String> {
        Err("Not implemented".to_string())
    }
}
