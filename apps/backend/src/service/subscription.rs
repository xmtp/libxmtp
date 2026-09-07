use crate::{Backend, api};
use futures::Stream;
use std::pin::Pin;
use tonic::{Request, Response, Status, Streaming};

#[tonic::async_trait]
impl api::subscription_service_server::SubscriptionService for Backend {
    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<api::SubscribeResponse, Status>> + Send>>;
    type SubscribeStaticStream =
        Pin<Box<dyn Stream<Item = Result<api::SubscribeStaticResponse, Status>> + Send>>;

    async fn subscribe(
        &self,
        _: Request<Streaming<api::SubscribeRequest>>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        Err(Status::unimplemented("subscriptions are not available"))
    }

    async fn subscribe_static(
        &self,
        _: Request<api::SubscribeStaticRequest>,
    ) -> Result<Response<Self::SubscribeStaticStream>, Status> {
        Err(Status::unimplemented("subscriptions are not available"))
    }
}
