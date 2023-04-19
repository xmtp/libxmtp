use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tonic::{metadata::MetadataValue, transport::Channel, Request, Streaming};
use xmtp_proto::xmtp::message_api::v1::{
    message_api_client::MessageApiClient, Envelope, PagingInfo, PublishRequest, PublishResponse,
    QueryRequest, QueryResponse, SubscribeRequest,
};

pub struct Client {
    client: MessageApiClient<Channel>,
    channel: Channel,
}

impl Client {
    pub async fn create(host: String) -> Result<Self, tonic::Status> {
        let host = host.to_string();
        let channel = Channel::from_shared(host)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?
            .connect()
            .await
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

        let client = MessageApiClient::new(channel.clone());

        Ok(Self { client, channel })
    }

    pub async fn publish(
        &mut self,
        token: String,
        envelopes: Vec<Envelope>,
    ) -> Result<PublishResponse, tonic::Status> {
        let auth_token_string = format!("Bearer {}", token);
        let token: MetadataValue<_> = auth_token_string
            .parse()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

        let mut client =
            MessageApiClient::with_interceptor(self.channel.clone(), move |mut req: Request<()>| {
                req.metadata_mut().insert("authorization", token.clone());
                Ok(req)
            });

        return client
            .publish(PublishRequest { envelopes })
            .await
            .map(|r| r.into_inner());
    }

    pub async fn subscribe(&mut self, topics: Vec<String>) -> Result<Subscription, tonic::Status> {
        let request = SubscribeRequest {
            content_topics: topics,
        };
        let stream = self.client.subscribe(request).await.unwrap().into_inner();

        return Ok(Subscription::start(stream).await);
    }

    pub async fn query(
        &mut self,
        topic: String,
        start_time: Option<u64>,
        end_time: Option<u64>,
        paging_info: Option<PagingInfo>,
    ) -> Result<QueryResponse, tonic::Status> {
        let request = QueryRequest {
            content_topics: vec![topic],
            start_time_ns: start_time.unwrap_or(0),
            end_time_ns: end_time.unwrap_or(0),
            paging_info,
        };

        match self.client.query(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(e),
        }
    }
}

pub struct Subscription {
    pending: Arc<Mutex<Vec<Envelope>>>,
    close_sender: Option<oneshot::Sender<()>>,
}

impl Subscription {
    pub async fn start(stream: Streaming<Envelope>) -> Self {
        let pending = Arc::new(Mutex::new(Vec::new()));
        let pending_clone = pending.clone();
        let (close_sender, close_receiver) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let mut stream = Box::pin(stream);
            let mut close_receiver = Box::pin(close_receiver);

            loop {
                tokio::select! {
                    item = stream.message() => {
                        match item {
                            Ok(Some(envelope)) => {
                                let mut pending = pending_clone.lock().unwrap();
                                pending.push(envelope);
                            }
                            _ => break,
                        }
                    },
                    _ = &mut close_receiver => {
                        break;
                    }
                }
            }
        });

        Subscription {
            pending,
            close_sender: Some(close_sender),
        }
    }

    pub fn get_messages(&self) -> Vec<Envelope> {
        let mut pending = self.pending.lock().unwrap();
        let items = pending.drain(..).collect::<Vec<Envelope>>();
        items
    }

    pub fn close_stream(&mut self) {
        if let Some(close_tx) = self.close_sender.take() {
            let _ = close_tx.send(());
        }
    }
}
