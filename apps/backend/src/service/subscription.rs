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

    /// Handle the bidirectional subscription endpoint.
    ///
    /// The session owns ordered interests and delivery until half-close or
    /// cancellation. Preserve the server-generated request identity and tracing
    /// context when the session moves into its background task.
    async fn subscribe(
        &self,
        request: Request<Streaming<api::SubscribeRequest>>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let hub = self
            .streams
            .clone()
            .ok_or_else(|| Status::unavailable("stream service unavailable"))?;
        let request_id = request
            .extensions()
            .get::<crate::server::telemetry::RequestId>()
            .map(|id| id.0)
            .unwrap_or_else(uuid::Uuid::new_v4);
        Ok(Response::new(Box::pin(crate::stream::native(
            hub,
            self.config.clone(),
            request.into_inner(),
            request_id,
        )?)))
    }

    /// Handle the static subscription endpoint.
    ///
    /// Static sessions share native ordering and capacity rules. The initial
    /// fixed targets precede data, and the request ending does not end delivery.
    async fn subscribe_static(
        &self,
        request: Request<api::SubscribeStaticRequest>,
    ) -> Result<Response<Self::SubscribeStaticStream>, Status> {
        let hub = self
            .streams
            .clone()
            .ok_or_else(|| Status::unavailable("stream service unavailable"))?;
        let request_id = request
            .extensions()
            .get::<crate::server::telemetry::RequestId>()
            .map(|id| id.0)
            .unwrap_or_else(uuid::Uuid::new_v4);
        Ok(Response::new(Box::pin(crate::stream::static_subscription(
            hub,
            self.config.clone(),
            request.into_inner().topics,
            request_id,
        )?)))
    }
}
