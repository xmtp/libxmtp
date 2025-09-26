use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::{
    VerifySmartContractWalletSignatureRequestSignature, VerifySmartContractWalletSignaturesRequest,
    VerifySmartContractWalletSignaturesResponse,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
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
        Cow::Borrowed("/identity/v1/verify-smart-contract-wallet-signatures")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<VerifySmartContractWalletSignaturesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(VerifySmartContractWalletSignaturesRequest {
            signatures: self.signatures.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<VerifySmartContractWalletSignaturesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_verify_smart_contract_wallet_signatures() {
        let client = crate::TestGrpcClient::create_local();
        let client = client.build().await.unwrap();
        let mut endpoint = VerifySmartContractWalletSignatures::builder()
            .signatures(vec![VerifySmartContractWalletSignatureRequestSignature {
                account_id: "".into(),
                block_number: None,
                hash: vec![],
                signature: vec![],
            }])
            .build()
            .unwrap();
        endpoint.query(&client).await.unwrap();
    }
}
