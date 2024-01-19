// @generated
/// Generated client implementations.
#[cfg(feature = "tonic")]
pub mod mls_api_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    #[derive(Debug, Clone)]
    pub struct MlsApiClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl MlsApiClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> MlsApiClient<T>
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
        ) -> MlsApiClient<InterceptedService<T, F>>
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
            MlsApiClient::new(InterceptedService::new(inner, interceptor))
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
        pub async fn send_group_messages(
            &mut self,
            request: impl tonic::IntoRequest<super::SendGroupMessagesRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status> {
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
                "/xmtp.mls.api.v1.MlsApi/SendGroupMessages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "SendGroupMessages"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn send_welcome_messages(
            &mut self,
            request: impl tonic::IntoRequest<super::SendWelcomeMessagesRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status> {
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
                "/xmtp.mls.api.v1.MlsApi/SendWelcomeMessages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "SendWelcomeMessages"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn register_installation(
            &mut self,
            request: impl tonic::IntoRequest<super::RegisterInstallationRequest>,
        ) -> std::result::Result<
            tonic::Response<super::RegisterInstallationResponse>,
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
                "/xmtp.mls.api.v1.MlsApi/RegisterInstallation",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "RegisterInstallation"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn upload_key_package(
            &mut self,
            request: impl tonic::IntoRequest<super::UploadKeyPackageRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status> {
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
                "/xmtp.mls.api.v1.MlsApi/UploadKeyPackage",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "UploadKeyPackage"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn fetch_key_packages(
            &mut self,
            request: impl tonic::IntoRequest<super::FetchKeyPackagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::FetchKeyPackagesResponse>,
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
                "/xmtp.mls.api.v1.MlsApi/FetchKeyPackages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "FetchKeyPackages"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn revoke_installation(
            &mut self,
            request: impl tonic::IntoRequest<super::RevokeInstallationRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status> {
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
                "/xmtp.mls.api.v1.MlsApi/RevokeInstallation",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "RevokeInstallation"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn get_identity_updates(
            &mut self,
            request: impl tonic::IntoRequest<super::GetIdentityUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetIdentityUpdatesResponse>,
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
                "/xmtp.mls.api.v1.MlsApi/GetIdentityUpdates",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "GetIdentityUpdates"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn query_group_messages(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryGroupMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryGroupMessagesResponse>,
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
                "/xmtp.mls.api.v1.MlsApi/QueryGroupMessages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "QueryGroupMessages"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn query_welcome_messages(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryWelcomeMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryWelcomeMessagesResponse>,
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
                "/xmtp.mls.api.v1.MlsApi/QueryWelcomeMessages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "QueryWelcomeMessages"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn subscribe_group_messages(
            &mut self,
            request: impl tonic::IntoRequest<super::SubscribeGroupMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<tonic::codec::Streaming<super::GroupMessage>>,
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
                "/xmtp.mls.api.v1.MlsApi/SubscribeGroupMessages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "SubscribeGroupMessages"),
                );
            self.inner.server_streaming(req, path, codec).await
        }
        pub async fn subscribe_welcome_messages(
            &mut self,
            request: impl tonic::IntoRequest<super::SubscribeWelcomeMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<tonic::codec::Streaming<super::WelcomeMessage>>,
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
                "/xmtp.mls.api.v1.MlsApi/SubscribeWelcomeMessages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.mls.api.v1.MlsApi", "SubscribeWelcomeMessages"),
                );
            self.inner.server_streaming(req, path, codec).await
        }
    }
}
/// Generated server implementations.
#[cfg(feature = "tonic")]
pub mod mls_api_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with MlsApiServer.
    #[async_trait]
    pub trait MlsApi: Send + Sync + 'static {
        async fn send_group_messages(
            &self,
            request: tonic::Request<super::SendGroupMessagesRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status>;
        async fn send_welcome_messages(
            &self,
            request: tonic::Request<super::SendWelcomeMessagesRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status>;
        async fn register_installation(
            &self,
            request: tonic::Request<super::RegisterInstallationRequest>,
        ) -> std::result::Result<
            tonic::Response<super::RegisterInstallationResponse>,
            tonic::Status,
        >;
        async fn upload_key_package(
            &self,
            request: tonic::Request<super::UploadKeyPackageRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status>;
        async fn fetch_key_packages(
            &self,
            request: tonic::Request<super::FetchKeyPackagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::FetchKeyPackagesResponse>,
            tonic::Status,
        >;
        async fn revoke_installation(
            &self,
            request: tonic::Request<super::RevokeInstallationRequest>,
        ) -> std::result::Result<tonic::Response<::pbjson_types::Empty>, tonic::Status>;
        async fn get_identity_updates(
            &self,
            request: tonic::Request<super::GetIdentityUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetIdentityUpdatesResponse>,
            tonic::Status,
        >;
        async fn query_group_messages(
            &self,
            request: tonic::Request<super::QueryGroupMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryGroupMessagesResponse>,
            tonic::Status,
        >;
        async fn query_welcome_messages(
            &self,
            request: tonic::Request<super::QueryWelcomeMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::QueryWelcomeMessagesResponse>,
            tonic::Status,
        >;
        /// Server streaming response type for the SubscribeGroupMessages method.
        type SubscribeGroupMessagesStream: futures_core::Stream<
                Item = std::result::Result<super::GroupMessage, tonic::Status>,
            >
            + Send
            + 'static;
        async fn subscribe_group_messages(
            &self,
            request: tonic::Request<super::SubscribeGroupMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::SubscribeGroupMessagesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the SubscribeWelcomeMessages method.
        type SubscribeWelcomeMessagesStream: futures_core::Stream<
                Item = std::result::Result<super::WelcomeMessage, tonic::Status>,
            >
            + Send
            + 'static;
        async fn subscribe_welcome_messages(
            &self,
            request: tonic::Request<super::SubscribeWelcomeMessagesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::SubscribeWelcomeMessagesStream>,
            tonic::Status,
        >;
    }
    #[derive(Debug)]
    pub struct MlsApiServer<T: MlsApi> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: MlsApi> MlsApiServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
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
    impl<T, B> tonic::codegen::Service<http::Request<B>> for MlsApiServer<T>
    where
        T: MlsApi,
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
            let inner = self.inner.clone();
            match req.uri().path() {
                "/xmtp.mls.api.v1.MlsApi/SendGroupMessages" => {
                    #[allow(non_camel_case_types)]
                    struct SendGroupMessagesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::SendGroupMessagesRequest>
                    for SendGroupMessagesSvc<T> {
                        type Response = ::pbjson_types::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SendGroupMessagesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).send_group_messages(request).await
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
                        let inner = inner.0;
                        let method = SendGroupMessagesSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/SendWelcomeMessages" => {
                    #[allow(non_camel_case_types)]
                    struct SendWelcomeMessagesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::SendWelcomeMessagesRequest>
                    for SendWelcomeMessagesSvc<T> {
                        type Response = ::pbjson_types::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SendWelcomeMessagesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).send_welcome_messages(request).await
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
                        let inner = inner.0;
                        let method = SendWelcomeMessagesSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/RegisterInstallation" => {
                    #[allow(non_camel_case_types)]
                    struct RegisterInstallationSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::RegisterInstallationRequest>
                    for RegisterInstallationSvc<T> {
                        type Response = super::RegisterInstallationResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RegisterInstallationRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).register_installation(request).await
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
                        let inner = inner.0;
                        let method = RegisterInstallationSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/UploadKeyPackage" => {
                    #[allow(non_camel_case_types)]
                    struct UploadKeyPackageSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::UploadKeyPackageRequest>
                    for UploadKeyPackageSvc<T> {
                        type Response = ::pbjson_types::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::UploadKeyPackageRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).upload_key_package(request).await
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
                        let inner = inner.0;
                        let method = UploadKeyPackageSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/FetchKeyPackages" => {
                    #[allow(non_camel_case_types)]
                    struct FetchKeyPackagesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::FetchKeyPackagesRequest>
                    for FetchKeyPackagesSvc<T> {
                        type Response = super::FetchKeyPackagesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::FetchKeyPackagesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).fetch_key_packages(request).await
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
                        let inner = inner.0;
                        let method = FetchKeyPackagesSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/RevokeInstallation" => {
                    #[allow(non_camel_case_types)]
                    struct RevokeInstallationSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::RevokeInstallationRequest>
                    for RevokeInstallationSvc<T> {
                        type Response = ::pbjson_types::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RevokeInstallationRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).revoke_installation(request).await
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
                        let inner = inner.0;
                        let method = RevokeInstallationSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/GetIdentityUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetIdentityUpdatesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::GetIdentityUpdatesRequest>
                    for GetIdentityUpdatesSvc<T> {
                        type Response = super::GetIdentityUpdatesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetIdentityUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).get_identity_updates(request).await
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
                        let inner = inner.0;
                        let method = GetIdentityUpdatesSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/QueryGroupMessages" => {
                    #[allow(non_camel_case_types)]
                    struct QueryGroupMessagesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::QueryGroupMessagesRequest>
                    for QueryGroupMessagesSvc<T> {
                        type Response = super::QueryGroupMessagesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryGroupMessagesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).query_group_messages(request).await
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
                        let inner = inner.0;
                        let method = QueryGroupMessagesSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/QueryWelcomeMessages" => {
                    #[allow(non_camel_case_types)]
                    struct QueryWelcomeMessagesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::UnaryService<super::QueryWelcomeMessagesRequest>
                    for QueryWelcomeMessagesSvc<T> {
                        type Response = super::QueryWelcomeMessagesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::QueryWelcomeMessagesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).query_welcome_messages(request).await
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
                        let inner = inner.0;
                        let method = QueryWelcomeMessagesSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/SubscribeGroupMessages" => {
                    #[allow(non_camel_case_types)]
                    struct SubscribeGroupMessagesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::ServerStreamingService<
                        super::SubscribeGroupMessagesRequest,
                    > for SubscribeGroupMessagesSvc<T> {
                        type Response = super::GroupMessage;
                        type ResponseStream = T::SubscribeGroupMessagesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SubscribeGroupMessagesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).subscribe_group_messages(request).await
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
                        let inner = inner.0;
                        let method = SubscribeGroupMessagesSvc(inner);
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
                "/xmtp.mls.api.v1.MlsApi/SubscribeWelcomeMessages" => {
                    #[allow(non_camel_case_types)]
                    struct SubscribeWelcomeMessagesSvc<T: MlsApi>(pub Arc<T>);
                    impl<
                        T: MlsApi,
                    > tonic::server::ServerStreamingService<
                        super::SubscribeWelcomeMessagesRequest,
                    > for SubscribeWelcomeMessagesSvc<T> {
                        type Response = super::WelcomeMessage;
                        type ResponseStream = T::SubscribeWelcomeMessagesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::SubscribeWelcomeMessagesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                (*inner).subscribe_welcome_messages(request).await
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
                        let inner = inner.0;
                        let method = SubscribeWelcomeMessagesSvc(inner);
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
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: MlsApi> Clone for MlsApiServer<T> {
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
    impl<T: MlsApi> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: MlsApi> tonic::server::NamedService for MlsApiServer<T> {
        const NAME: &'static str = "xmtp.mls.api.v1.MlsApi";
    }
}
