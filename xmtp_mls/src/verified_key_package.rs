use openmls::prelude::{KeyPackage, KeyPackageIn, KeyPackageVerifyError};
use openmls_traits::OpenMlsProvider;
use prost::{DecodeError, Message};
use thiserror::Error;
use tls_codec::{Deserialize, Error as TlsSerializationError};
use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;

use crate::{
    association::{AssociationError, Eip191Association},
    configuration::MLS_PROTOCOL_VERSION,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Debug, Error)]
pub enum KeyPackageVerificationError {
    #[error("serialization error: {0}")]
    Serialization(#[from] TlsSerializationError),
    #[error("mls validation: {0}")]
    MlsValidation(#[from] KeyPackageVerifyError),
    #[error("association: {0}")]
    Association(#[from] AssociationError),
    #[error("decode: {0}")]
    Decode(#[from] DecodeError),
    #[error("generic: {0}")]
    Generic(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedKeyPackage {
    pub inner: KeyPackage,
    pub wallet_address: String,
}

impl VerifiedKeyPackage {
    pub fn new(inner: KeyPackage, wallet_address: String) -> Self {
        Self {
            inner,
            wallet_address,
        }
    }

    // Validates starting with a KeyPackage (which is already validated by OpenMLS)
    pub fn from_key_package(kp: KeyPackage) -> Result<Self, KeyPackageVerificationError> {
        let leaf_node = kp.leaf_node();
        let identity_bytes = leaf_node.credential().identity();
        let pub_key_bytes = leaf_node.signature_key().as_slice();
        let wallet_address = identity_to_wallet_address(identity_bytes, pub_key_bytes)?;

        Ok(Self::new(kp, wallet_address))
    }

    // Validates starting with a KeyPackageIn as bytes (which is not validated by OpenMLS)
    pub fn from_bytes(
        mls_provider: &XmtpOpenMlsProvider,
        data: &[u8],
    ) -> Result<VerifiedKeyPackage, KeyPackageVerificationError> {
        let kp_in: KeyPackageIn = KeyPackageIn::tls_deserialize_bytes(data)?;
        let kp = kp_in.validate(mls_provider.crypto(), MLS_PROTOCOL_VERSION)?;

        Self::from_key_package(kp)
    }

    pub fn installation_id(&self) -> Vec<u8> {
        self.inner.leaf_node().signature_key().as_slice().to_vec()
    }
}

fn identity_to_wallet_address(
    identity_bytes: &[u8],
    pub_key_bytes: &[u8],
) -> Result<String, KeyPackageVerificationError> {
    let proto_value = Eip191AssociationProto::decode(identity_bytes)?;
    let association = Eip191Association::from_proto_with_expected_address(
        pub_key_bytes,
        proto_value.clone(),
        proto_value.wallet_address,
    )?;

    Ok(association.address())
}
