use std::collections::HashMap;

use xmtp_networking::grpc_api_helper::Subscription;
use xmtp_proto::xmtp::message_api::v1::{Envelope, PagingInfo, PublishResponse, QueryResponse};

pub trait XmtpApiClient {
    fn publish(
        &mut self,
        token: String,
        envelopes: Vec<Envelope>,
        // TODO: use error enums
    ) -> Result<PublishResponse, String>;

    fn query(
        self,
        topic: String,
        start_time: Option<u64>,
        end_time: Option<u64>,
        paging_info: Option<PagingInfo>,
        // TODO: use error enums
    ) -> Result<QueryResponse, String>;

    // TODO: use error enums
    fn subscribe(self, topics: Vec<String>) -> Result<Subscription, String>;
}

pub struct MockXmtpApiClient {
    messages: HashMap<String, Vec<Envelope>>,
}

/**
 * Temporarily adding this so I don't have to deal with Tonic issues making something work for tests
 * TODO: Replace me with real networking client
 */
impl MockXmtpApiClient {
    pub fn new() -> Self {
        Self {
            messages: HashMap::new(),
        }
    }
}

impl XmtpApiClient for MockXmtpApiClient {
    fn publish(
        &mut self,
        token: String,
        envelopes: Vec<Envelope>,
    ) -> Result<PublishResponse, String> {
        let mut existing: Vec<Envelope> = match self.messages.get(&token) {
            Some(envelopes) => envelopes.clone(),
            None => vec![],
        };
        existing.append(envelopes.clone().as_mut());
        self.messages.insert(token, envelopes);

        Ok(PublishResponse {})
    }

    fn query(
        self,
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
            envelopes: envelopes,
            paging_info: None,
        })
    }

    fn subscribe(self, _topics: Vec<String>) -> Result<Subscription, String> {
        Err("Not implemented".to_string())
    }
}
