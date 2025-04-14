use openmls_rust_crypto::RustCrypto;
use xmtp_mls::verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2};

pub fn validate_inbox_id_key_package(key_package: Vec<u8>) -> Result<(), KeyPackageVerificationError> {
    let rust_crypto = RustCrypto::default();
    VerifiedKeyPackageV2::from_bytes(&rust_crypto, key_package.as_slice()).map(|_| ())
}