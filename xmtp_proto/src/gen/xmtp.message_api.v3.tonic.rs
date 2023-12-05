// @generated
/// Generated client implementations.
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
        pub async fn publish_to_group(
            &mut self,
            request: impl tonic::IntoRequest<super::PublishToGroupRequest>,
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
                "/xmtp.message_api.v3.MlsApi/PublishToGroup",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("xmtp.message_api.v3.MlsApi", "PublishToGroup"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn publish_welcomes(
            &mut self,
            request: impl tonic::IntoRequest<super::PublishWelcomesRequest>,
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
                "/xmtp.message_api.v3.MlsApi/PublishWelcomes",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.message_api.v3.MlsApi", "PublishWelcomes"),
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
                "/xmtp.message_api.v3.MlsApi/RegisterInstallation",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.message_api.v3.MlsApi", "RegisterInstallation"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn upload_key_packages(
            &mut self,
            request: impl tonic::IntoRequest<super::UploadKeyPackagesRequest>,
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
                "/xmtp.message_api.v3.MlsApi/UploadKeyPackages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.message_api.v3.MlsApi", "UploadKeyPackages"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn consume_key_packages(
            &mut self,
            request: impl tonic::IntoRequest<super::ConsumeKeyPackagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ConsumeKeyPackagesResponse>,
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
                "/xmtp.message_api.v3.MlsApi/ConsumeKeyPackages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.message_api.v3.MlsApi", "ConsumeKeyPackages"),
                );
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
                "/xmtp.message_api.v3.MlsApi/RevokeInstallation",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.message_api.v3.MlsApi", "RevokeInstallation"),
                );
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
                "/xmtp.message_api.v3.MlsApi/GetIdentityUpdates",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("xmtp.message_api.v3.MlsApi", "GetIdentityUpdates"),
                );
            self.inner.unary(req, path, codec).await
        }
    }
}
