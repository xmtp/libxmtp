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

    // TODO(cvoell): implement test
    #[xmtp_common::test]
    fn test_file_descriptor() {
        // stub for now
    }

    // TODO(cvoell): implement test
    #[xmtp_common::test]
    async fn test_query_commit_log() {
        // stub for now
    }
}
