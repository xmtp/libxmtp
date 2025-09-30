use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HealthCheckRequest {
    #[prost(string, tag = "1")]
    pub service: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HealthCheckResponse {
    #[prost(enumeration = "ServingStatus", tag = "1")]
    pub status: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ServingStatus {
    Unknown = 0,
    Serving = 1,
    NotServing = 2,
    ServiceUnknown = 3,
}

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct HealthCheck {
    #[builder(setter(into), default)]
    service: String,
}

impl HealthCheck {
    pub fn builder() -> HealthCheckBuilder {
        Default::default()
    }
}

impl Endpoint for HealthCheck {
    type Output = HealthCheckResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/grpc.health.v1.Health/Check")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/grpc.health.v1.Health/Check")
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(HealthCheckRequest {
            service: self.service.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::d14n::HealthCheck;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    async fn test_health_check() {
        let client = crate::TestClient::create_local_d14n();
        let client = client.build().await.unwrap();
        let endpoint = HealthCheck::builder().build().unwrap();
        assert!(endpoint.query(&client).await.is_ok());
    }
}
