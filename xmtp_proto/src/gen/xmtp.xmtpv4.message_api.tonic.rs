// @generated
/// Generated client implementations.
pub mod replication_api_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    ///
    #[derive(Debug, Clone)]
    pub struct ReplicationApiClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl<T> ReplicationApiClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> ReplicationApiClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            ReplicationApiClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        ///
        pub async fn subscribe_envelopes(
            &mut self,
            request: impl tonic::IntoRequest<super::SubscribeEnvelopesRequest>,
        ) -> std::result::Result<
            tonic::Response<tonic::codec::Streaming<super::SubscribeEnvelopesResponse>>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeEnvelopes",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "xmtp.xmtpv4.message_api.ReplicationApi",
                        "SubscribeEnvelopes",
                    ),
                );
            self.inner.server_streaming(req, path, codec).await
        }
        ///
        pub async fn query_envelopes(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryEnvelopesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryEnvelopesResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "xmtp.xmtpv4.message_api.ReplicationApi",
                        "QueryEnvelopes",
                    ),
                );
            self.inner.unary(req, path, codec).await
        }
        ///
        pub async fn publish_payer_envelopes(
            &mut self,
            request: impl tonic::IntoRequest<super::PublishPayerEnvelopesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::PublishPayerEnvelopesResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/xmtp.xmtpv4.message_api.ReplicationApi/PublishPayerEnvelopes",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "xmtp.xmtpv4.message_api.ReplicationApi",
                        "PublishPayerEnvelopes",
                    ),
                );
            self.inner.unary(req, path, codec).await
        }
        ///
        pub async fn get_inbox_ids(
            &mut self,
            request: impl tonic::IntoRequest<super::GetInboxIdsRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetInboxIdsResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/xmtp.xmtpv4.message_api.ReplicationApi/GetInboxIds",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "xmtp.xmtpv4.message_api.ReplicationApi",
                        "GetInboxIds",
                    ),
                );
            self.inner.unary(req, path, codec).await
        }
        /** Get the newest envelope for each topic
*/
        pub async fn get_newest_envelope(
            &mut self,
            request: impl tonic::IntoRequest<super::GetNewestEnvelopeRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetNewestEnvelopeResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/xmtp.xmtpv4.message_api.ReplicationApi/GetNewestEnvelope",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "xmtp.xmtpv4.message_api.ReplicationApi",
                        "GetNewestEnvelope",
                    ),
                );
            self.inner.unary(req, path, codec).await
        }
    }
}
/// Generated server implementations.
#[cfg(not(target_arch = "wasm32"))]
pub mod replication_api_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with ReplicationApiServer.
    #[async_trait]
    pub trait ReplicationApi: Send + Sync + 'static {
        /// Server streaming response type for the SubscribeEnvelopes method.
        type SubscribeEnvelopesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::SubscribeEnvelopesResponse,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        ///
        async fn subscribe_envelopes(
            &self,
            request: tonic::Request<super::SubscribeEnvelopesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::SubscribeEnvelopesStream>,
            tonic::Status,
        >;
        ///
        async fn query_envelopes(
            &self,
            request: tonic::Request<super::QueryEnvelopesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryEnvelopesResponse>,
            tonic::Status,
        >;
        ///
        async fn publish_payer_envelopes(
            &self,
            request: tonic::Request<super::PublishPayerEnvelopesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::PublishPayerEnvelopesResponse>,
            tonic::Status,
        >;
        ///
        async fn get_inbox_ids(
            &self,
            request: tonic::Request<super::GetInboxIdsRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetInboxIdsResponse>,
            tonic::Status,
        >;
        /** Get the newest envelope for each topic
*/
        async fn get_newest_envelope(
            &self,
            request: tonic::Request<super::GetNewestEnvelopeRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetNewestEnvelopeResponse>,
            tonic::Status,
        >;
    }
    ///
    #[derive(Debug)]
    pub struct ReplicationApiServer<T: ReplicationApi> {
        inner: Arc<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    impl<T: ReplicationApi> ReplicationApiServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
                max_decoding_message_size: None,
                max_encoding_message_size: None,
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.max_decoding_message_size = Some(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.max_encoding_message_size = Some(limit);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for ReplicationApiServer<T>
    where
        T: ReplicationApi,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            match req.uri().path() {
                "/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeEnvelopes" => {
                    #[allow(non_camel_case_types)]
                    struct SubscribeEnvelopesSvc<T: ReplicationApi>(pub Arc<T>);
                    impl<
                        T: ReplicationApi,
                    > tonic::server::ServerStreamingService<
                        super::SubscribeEnvelopesRequest,
                    > for SubscribeEnvelopesSvc<T> {
                        type Response = super::SubscribeEnvelopesResponse;
                        type ResponseStream = T::SubscribeEnvelopesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SubscribeEnvelopesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as ReplicationApi>::subscribe_envelopes(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = SubscribeEnvelopesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes" => {
                    #[allow(non_camel_case_types)]
                    struct QueryEnvelopesSvc<T: ReplicationApi>(pub Arc<T>);
                    impl<
                        T: ReplicationApi,
                    > tonic::server::UnaryService<super::QueryEnvelopesRequest>
                    for QueryEnvelopesSvc<T> {
                        type Response = super::QueryEnvelopesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryEnvelopesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as ReplicationApi>::query_envelopes(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = QueryEnvelopesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/xmtp.xmtpv4.message_api.ReplicationApi/PublishPayerEnvelopes" => {
                    #[allow(non_camel_case_types)]
                    struct PublishPayerEnvelopesSvc<T: ReplicationApi>(pub Arc<T>);
                    impl<
                        T: ReplicationApi,
                    > tonic::server::UnaryService<super::PublishPayerEnvelopesRequest>
                    for PublishPayerEnvelopesSvc<T> {
                        type Response = super::PublishPayerEnvelopesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::PublishPayerEnvelopesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as ReplicationApi>::publish_payer_envelopes(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = PublishPayerEnvelopesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/xmtp.xmtpv4.message_api.ReplicationApi/GetInboxIds" => {
                    #[allow(non_camel_case_types)]
                    struct GetInboxIdsSvc<T: ReplicationApi>(pub Arc<T>);
                    impl<
                        T: ReplicationApi,
                    > tonic::server::UnaryService<super::GetInboxIdsRequest>
                    for GetInboxIdsSvc<T> {
                        type Response = super::GetInboxIdsResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetInboxIdsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as ReplicationApi>::get_inbox_ids(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = GetInboxIdsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/xmtp.xmtpv4.message_api.ReplicationApi/GetNewestEnvelope" => {
                    #[allow(non_camel_case_types)]
                    struct GetNewestEnvelopeSvc<T: ReplicationApi>(pub Arc<T>);
                    impl<
                        T: ReplicationApi,
                    > tonic::server::UnaryService<super::GetNewestEnvelopeRequest>
                    for GetNewestEnvelopeSvc<T> {
                        type Response = super::GetNewestEnvelopeResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetNewestEnvelopeRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as ReplicationApi>::get_newest_envelope(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = GetNewestEnvelopeSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", tonic::Code::Unimplemented as i32)
                                .header(
                                    http::header::CONTENT_TYPE,
                                    tonic::metadata::GRPC_CONTENT_TYPE,
                                )
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: ReplicationApi> Clone for ReplicationApiServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
                max_decoding_message_size: self.max_decoding_message_size,
                max_encoding_message_size: self.max_encoding_message_size,
            }
        }
    }
    impl<T: ReplicationApi> tonic::server::NamedService for ReplicationApiServer<T> {
        const NAME: &'static str = "xmtp.xmtpv4.message_api.ReplicationApi";
    }
}
/// Generated client implementations.
pub mod misbehavior_api_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    ///
    #[derive(Debug, Clone)]
    pub struct MisbehaviorApiClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl<T> MisbehaviorApiClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> MisbehaviorApiClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            MisbehaviorApiClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        ///
        pub async fn submit_misbehavior_report(
            &mut self,
            request: impl tonic::IntoRequest<super::SubmitMisbehaviorReportRequest>,
        ) -> std::result::Result<
            tonic::Response<super::SubmitMisbehaviorReportResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/xmtp.xmtpv4.message_api.MisbehaviorApi/SubmitMisbehaviorReport",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "xmtp.xmtpv4.message_api.MisbehaviorApi",
                        "SubmitMisbehaviorReport",
                    ),
                );
            self.inner.unary(req, path, codec).await
        }
        ///
        pub async fn query_misbehavior_reports(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryMisbehaviorReportsRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryMisbehaviorReportsResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/xmtp.xmtpv4.message_api.MisbehaviorApi/QueryMisbehaviorReports",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "xmtp.xmtpv4.message_api.MisbehaviorApi",
                        "QueryMisbehaviorReports",
                    ),
                );
            self.inner.unary(req, path, codec).await
        }
    }
}
/// Generated server implementations.
#[cfg(not(target_arch = "wasm32"))]
pub mod misbehavior_api_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with MisbehaviorApiServer.
    #[async_trait]
    pub trait MisbehaviorApi: Send + Sync + 'static {
        ///
        async fn submit_misbehavior_report(
            &self,
            request: tonic::Request<super::SubmitMisbehaviorReportRequest>,
        ) -> std::result::Result<
            tonic::Response<super::SubmitMisbehaviorReportResponse>,
            tonic::Status,
        >;
        ///
        async fn query_misbehavior_reports(
            &self,
            request: tonic::Request<super::QueryMisbehaviorReportsRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryMisbehaviorReportsResponse>,
            tonic::Status,
        >;
    }
    ///
    #[derive(Debug)]
    pub struct MisbehaviorApiServer<T: MisbehaviorApi> {
        inner: Arc<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    impl<T: MisbehaviorApi> MisbehaviorApiServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
                max_decoding_message_size: None,
                max_encoding_message_size: None,
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.max_decoding_message_size = Some(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.max_encoding_message_size = Some(limit);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for MisbehaviorApiServer<T>
    where
        T: MisbehaviorApi,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            match req.uri().path() {
                "/xmtp.xmtpv4.message_api.MisbehaviorApi/SubmitMisbehaviorReport" => {
                    #[allow(non_camel_case_types)]
                    struct SubmitMisbehaviorReportSvc<T: MisbehaviorApi>(pub Arc<T>);
                    impl<
                        T: MisbehaviorApi,
                    > tonic::server::UnaryService<super::SubmitMisbehaviorReportRequest>
                    for SubmitMisbehaviorReportSvc<T> {
                        type Response = super::SubmitMisbehaviorReportResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::SubmitMisbehaviorReportRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as MisbehaviorApi>::submit_misbehavior_report(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = SubmitMisbehaviorReportSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/xmtp.xmtpv4.message_api.MisbehaviorApi/QueryMisbehaviorReports" => {
                    #[allow(non_camel_case_types)]
                    struct QueryMisbehaviorReportsSvc<T: MisbehaviorApi>(pub Arc<T>);
                    impl<
                        T: MisbehaviorApi,
                    > tonic::server::UnaryService<super::QueryMisbehaviorReportsRequest>
                    for QueryMisbehaviorReportsSvc<T> {
                        type Response = super::QueryMisbehaviorReportsResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::QueryMisbehaviorReportsRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as MisbehaviorApi>::query_misbehavior_reports(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = QueryMisbehaviorReportsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", tonic::Code::Unimplemented as i32)
                                .header(
                                    http::header::CONTENT_TYPE,
                                    tonic::metadata::GRPC_CONTENT_TYPE,
                                )
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: MisbehaviorApi> Clone for MisbehaviorApiServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
                max_decoding_message_size: self.max_decoding_message_size,
                max_encoding_message_size: self.max_encoding_message_size,
            }
        }
    }
    impl<T: MisbehaviorApi> tonic::server::NamedService for MisbehaviorApiServer<T> {
        const NAME: &'static str = "xmtp.xmtpv4.message_api.MisbehaviorApi";
    }
}
