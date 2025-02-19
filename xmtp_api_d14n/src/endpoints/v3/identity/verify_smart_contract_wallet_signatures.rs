use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::{
    VerifySmartContractWalletSignatureRequestSignature, VerifySmartContractWalletSignaturesRequest,
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
    type Output = ();
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
