use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

#[derive(Clone, Debug)]
pub struct VerifySmartContractWalletSignatures(
    pub backend_v1::VerifySmartContractWalletSignaturesRequest,
);

impl Endpoint for VerifySmartContractWalletSignatures {
    type Output = backend_v1::VerifySmartContractWalletSignaturesResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        "/xmtp.backend.v1.IdentityService/VerifySmartContractWalletSignatures".into()
    }
    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
