use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::mls_v1::BatchQueryCommitLogResponse;
use xmtp_proto::xmtp::mls::api::v1::{BatchQueryCommitLogRequest, QueryCommitLogRequest};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct QueryCommitLog {
    #[builder(setter(into))]
    query_log_requests: Vec<QueryCommitLogRequest>,
}

impl QueryCommitLog {
    pub fn builder() -> QueryCommitLogBuilder {
        Default::default()
    }
}

impl Endpoint for QueryCommitLog {
    type Output = BatchQueryCommitLogResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<BatchQueryCommitLogRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(BatchQueryCommitLogRequest {
            requests: self.query_log_requests.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_common::rand_vec;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<BatchQueryCommitLogRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = QueryCommitLog::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/BatchQueryCommitLog"
        );
    }

    #[xmtp_common::test]
    async fn test_query_commit_log() {
        let client = crate::TestGrpcClient::create_local();
        let client = client.build().unwrap();
        let endpoint = QueryCommitLog::builder()
            .query_log_requests(vec![QueryCommitLogRequest {
                group_id: rand_vec::<16>(),
                paging_info: None,
            }])
            .build()
            .unwrap();

        let result = xmtp_proto::api::ignore(endpoint).query(&client).await;
        assert!(result.is_ok(), "{:?}", result);
    }
}
