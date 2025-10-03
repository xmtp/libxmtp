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
    use xmtp_proto::xmtp::mls::api::v1::*;
    use xmtp_proto::{api, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<BatchPublishCommitLogRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_publish_commit_log() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let endpoint = PublishCommitLog::builder()
            .commit_log_entries(vec![PublishCommitLogRequest::default()])
            .build()
            .unwrap();

        let result = api::ignore(endpoint).query(&client).await;
        assert!(result.is_err());
    }
}
