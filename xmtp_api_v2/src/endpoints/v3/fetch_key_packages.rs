use derive_builder::Builder;
use prost::Message;
use prost_types::{FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto};
use std::borrow::Cow;
use std::fmt::Write;
use xmtp_proto::traits::{BodyError, Endpoint, Query};
use xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest;
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;

#[derive(Debug, Builder)]
#[builder(setter(strip_option))]
pub struct FetchKeyPackages {
    #[builder(setter(into))]
    installation_keys: Vec<Vec<u8>>,
}

impl Endpoint for FetchKeyPackages {
    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<FetchKeyPackagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(FetchKeyPackagesRequest {
            installation_keys: self.installation_keys.clone(),
        }
        .encode_to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<FetchKeyPackagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }
}
