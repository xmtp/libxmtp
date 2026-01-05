use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::mls_v1::{BatchPublishCommitLogRequest, PublishCommitLogRequest};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct PublishCommitLog {
    #[builder(setter(into))]
    commit_log_entries: Vec<PublishCommitLogRequest>,
}

impl PublishCommitLog {
    pub fn builder() -> PublishCommitLogBuilder {
        Default::default()
    }
}

impl Endpoint for PublishCommitLog {
    type Output = ();
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<BatchPublishCommitLogRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(BatchPublishCommitLogRequest {
            requests: self.commit_log_entries.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::v3::PublishCommitLog;
    use xmtp_api_grpc::error::GrpcError;
    use xmtp_api_grpc::test::NodeGoClient;
    use xmtp_common::rand_vec;
    use xmtp_proto::xmtp::mls::api::v1::*;
    use xmtp_proto::{api, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<BatchPublishCommitLogRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = PublishCommitLog::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/BatchPublishCommitLog"
        );
    }

    #[xmtp_common::test]
    async fn test_publish_commit_log() {
        let client = NodeGoClient::create();
        let client = client.build().unwrap();
        let endpoint = PublishCommitLog::builder()
            .commit_log_entries(vec![PublishCommitLogRequest {
                group_id: rand_vec::<16>(),
                serialized_commit_log_entry: rand_vec::<32>(),
                signature: None,
            }])
            .build()
            .unwrap();

        let err = api::ignore(endpoint).query(&client).await.unwrap_err();
        // the request will fail b/c we're using dummy data but
        // we just care if the endpoint is working
        match err {
            ApiClientError::<GrpcError>::ClientWithEndpoint {
                source: GrpcError::Status(ref s),
                ..
            } => {
                assert!(s.message().contains("invalid commit log entry"), "{}", err);
            }
            _ => panic!("request failed"),
        }
    }
}
