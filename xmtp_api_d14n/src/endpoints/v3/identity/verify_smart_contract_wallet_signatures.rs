use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::{
    VerifySmartContractWalletSignatureRequestSignature, VerifySmartContractWalletSignaturesRequest,
    VerifySmartContractWalletSignaturesResponse,
};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct VerifySmartContractWalletSignatures {
    #[builder(setter(into))]
    pub signatures: Vec<VerifySmartContractWalletSignatureRequestSignature>,
}

impl VerifySmartContractWalletSignatures {
    pub fn builder() -> VerifySmartContractWalletSignaturesBuilder {
        Default::default()
    }
}

impl Endpoint for VerifySmartContractWalletSignatures {
    type Output = VerifySmartContractWalletSignaturesResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<VerifySmartContractWalletSignaturesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(VerifySmartContractWalletSignaturesRequest {
            signatures: self.signatures.clone(),
        }
        .encode_to_vec())
    }
}

#[cfg(test)]
mod test {
    use crate::VerifySmartContractWalletSignatures;
    use xmtp_api_grpc::grpc_client::GrpcClient;
    use xmtp_api_grpc::LOCALHOST_ADDRESS;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;
    use xmtp_proto::xmtp::identity::api::v1::{
        VerifySmartContractWalletSignatureRequestSignature,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
        FILE_DESCRIPTOR_SET,
    };

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<VerifySmartContractWalletSignaturesRequest>(
            FILE_DESCRIPTOR_SET,
        );
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_get_identity_updates_v2() {
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();
        let endpoint = VerifySmartContractWalletSignatures::builder()
            .signatures(vec![VerifySmartContractWalletSignatureRequestSignature {
                account_id: "".into(),
                block_number: None,
                hash: vec![],
                signature: vec![],
            }])
            .build()
            .unwrap();

        let result: VerifySmartContractWalletSignaturesResponse =
            endpoint.query(&client).await.unwrap();
        assert_eq!(result.responses.len(), 0);
    }
}
