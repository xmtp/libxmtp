use openmls::prelude::{KeyPackage, KeyPackageIn, KeyPackageVerifyError};
use openmls_traits::OpenMlsProvider;
use thiserror::Error;
use tls_codec::{Deserialize, Error as TlsSerializationError};

use crate::{
    configuration::MLS_PROTOCOL_VERSION,
    identity::{Identity, IdentityError},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Debug, Error)]
pub enum KeyPackageVerificationError {
    #[error("serialization error: {0}")]
    Serialization(#[from] TlsSerializationError),
    #[error("mls validation: {0}")]
    MlsValidation(#[from] KeyPackageVerifyError),
    #[error("identity: {0}")]
    Identity(#[from] IdentityError),
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
    credential_bytes: &[u8],
    installation_key_bytes: &[u8],
) -> Result<String, KeyPackageVerificationError> {
    Ok(Identity::get_validated_account_address(
        credential_bytes,
        installation_key_bytes,
    )?)
}
