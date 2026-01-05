use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};

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

#[derive(Builder, Clone, Debug, Default)]
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
    use xmtp_api_grpc::test::{GatewayClient, XmtpdClient};
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    async fn test_health_check() {
        let mut endpoint = HealthCheck::builder().build().unwrap();

        let xmtpd_client = XmtpdClient::create();
        let client = xmtpd_client.build().unwrap();
        assert!(endpoint.query(&client).await.is_ok());

        let gateway_client = GatewayClient::create();
        let client = gateway_client.build().unwrap();
        assert!(endpoint.query(&client).await.is_ok());
    }
}
