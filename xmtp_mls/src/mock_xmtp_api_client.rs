use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use xmtp_proto::api_client::*;

pub struct MockXmtpApiSubscription {}

impl XmtpApiSubscription for MockXmtpApiSubscription {
    fn is_closed(&self) -> bool {
        false
    }

    fn get_messages(&self) -> Vec<Envelope> {
        vec![]
    }

    fn close_stream(&mut self) {}
}

#[derive(Debug)]
struct InnerMockXmtpApiClient {
    pub messages: HashMap<String, Vec<Envelope>>,
    pub app_version: String,
}

#[derive(Debug)]
pub struct MockXmtpApiClient {
    inner_client: Arc<Mutex<InnerMockXmtpApiClient>>,
}

impl MockXmtpApiClient {
    pub fn new() -> Self {
        Self {
            inner_client: Arc::new(Mutex::new(InnerMockXmtpApiClient {
                messages: HashMap::new(),
                app_version: String::from("0.0.0"),
            })),
        }
    }
}

impl Default for MockXmtpApiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MockXmtpApiClient {
    fn clone(&self) -> Self {
        Self {
            inner_client: self.inner_client.clone(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl XmtpApiClient for MockXmtpApiClient {
    type Subscription = MockXmtpApiSubscription;

    fn set_app_version(&mut self, version: String) {
        let mut inner = self.inner_client.lock().unwrap();
        inner.app_version = version;
    }

    async fn publish(
        &self,
        _token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error> {
        let mut inner = self.inner_client.lock().unwrap();
        for envelope in request.envelopes {
            let topic = envelope.content_topic.clone();
            let mut existing: Vec<Envelope> = match inner.messages.get(&topic) {
                Some(existing_envelopes) => existing_envelopes.clone(),
                None => vec![],
            };
            existing.push(envelope);
            inner.messages.insert(topic, existing);
        }
        Ok(PublishResponse {})
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error> {
        let inner = self.inner_client.lock().unwrap();
        let envelopes: Vec<Envelope> = match inner.messages.get(&request.content_topics[0]) {
            Some(envelopes) => envelopes.clone(),
            None => vec![],
        };

        Ok(QueryResponse {
            envelopes,
            paging_info: None,
        })
    }

    async fn subscribe(&self, _request: SubscribeRequest) -> Result<Self::Subscription, Error> {
        Err(Error::new(ErrorKind::SubscribeError))
    }

    async fn batch_query(&self, _request: BatchQueryRequest) -> Result<BatchQueryResponse, Error> {
        Err(Error::new(ErrorKind::BatchQueryError))
    }
}
