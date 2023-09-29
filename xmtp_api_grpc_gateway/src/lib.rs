use async_trait::async_trait;
use xmtp_proto::api_client::{Error, ErrorKind, XmtpApiClient, XmtpApiSubscription};
use xmtp_proto::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, Envelope, PublishRequest, PublishResponse, QueryRequest,
    QueryResponse, SubscribeRequest,
};

// TODO: consider moving these (and other address const) into `xmtp_proto`
pub const LOCALHOST_ADDRESS: &str = "http://localhost:5555";
pub const DEV_ADDRESS: &str = "https://dev.xmtp.network:5555";

pub struct XmtpGrpcGatewayClient {
    url: String,
    http: reqwest::Client,
}

impl XmtpGrpcGatewayClient {
    pub fn new(url: String) -> Self {
        XmtpGrpcGatewayClient {
            url,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait(?Send)]
impl XmtpApiClient for XmtpGrpcGatewayClient {
    type Subscription = XmtpGrpcGatewaySubscription;

    fn set_app_version(&mut self, _version: String) {
        // TODO
    }

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error> {
        let res: PublishResponse = self
            .http
            .post(&format!("{}/message/v1/publish", self.url))
            .bearer_auth(token)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::PublishError).with(e))?
            .json()
            .await
            .map_err(|e| Error::new(ErrorKind::PublishError).with(e))?;
        Ok(res)
    }

    async fn subscribe(
        &self,
        _request: SubscribeRequest,
    ) -> Result<XmtpGrpcGatewaySubscription, Error> {
        // TODO
        Err(Error::new(ErrorKind::SubscribeError))
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error> {
        let res: QueryResponse = self
            .http
            .post(&format!("{}/message/v1/query", self.url))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::QueryError).with(e))?
            .json()
            .await
            .map_err(|e| Error::new(ErrorKind::QueryError).with(e))?;
        Ok(res)
    }

    async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error> {
        let res: BatchQueryResponse = self
            .http
            .post(&format!("{}/message/v1/batch-query", self.url))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::BatchQueryError).with(e))?
            .json()
            .await
            .map_err(|e| Error::new(ErrorKind::BatchQueryError).with(e))?;
        Ok(res)
    }
}

// TODO: implement JSON segmented streaming
pub struct XmtpGrpcGatewaySubscription {}
impl XmtpApiSubscription for XmtpGrpcGatewaySubscription {
    fn is_closed(&self) -> bool {
        true
    }

    fn get_messages(&self) -> Vec<Envelope> {
        vec![]
    }

    fn close_stream(&mut self) {}
}
