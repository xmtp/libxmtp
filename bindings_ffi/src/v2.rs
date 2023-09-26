use core::slice::SlicePattern;

use crate::{stringify_error_chain, GenericError};

#[uniffi::export]
pub fn recover_address(
    signature_bytes: Vec<u8>,
    predigest_message: String,
) -> Result<String, GenericError> {
    let signature =
        xmtp_cryptography::signature::RecoverableSignature::Eip191Signature(signature_bytes);
    let recovered = signature
        .recover_address(&predigest_message)
        .map_err(|e| stringify_error_chain(&e))?;

    return Ok(recovered);
}

#[uniffi::export]
pub fn diffie_hellman_k256(
    private_key_bytes: Vec<u8>,
    public_key_bytes: Vec<u8>,
) -> Result<Vec<u8>, GenericError> {
    let shared_secret = xmtp_v2::k256_helper::diffie_hellman_byte_params(
        private_key_bytes.as_slice(),
        public_key_bytes.as_slice(),
    )
    .map_err(|e| e)?;
    Ok(shared_secret)
}

#[uniffi::export]
pub fn verify_k256_sha256(
    signed_by: Vec<u8>,
    message: Vec<u8>,
    signature: Vec<u8>,
    recovery_id: u8,
) -> Result<bool, GenericError> {
    let result = xmtp_v2::k256_helper::verify_sha256(
        signed_by.as_slice(),
        message.as_slice(),
        signature.as_slice(),
        recovery_id,
    )
    .map_err(|e| e)?;

    return Ok(result);
}

#[cfg(test)]
mod tests {
    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_recover_public_key_keccak256() {
        // This test was generated using Etherscans Signature tool: https://etherscan.io/verifySig/18959
        let addr = "0x1B2a516d691aBb8f08a75B2C73c95c62A1632431";
        let msg = "TestVector1";
        let sig_hash = "19d6bec562518e365d07ba3cce26d08a5fffa2cbb1e7fe03c1f2d6a722fd3a5e544097b91f8f8cd11d43b032659f30529139ab1a9ecb6c81ed4a762179e87db81c";

        let sig_bytes = ethers_core::utils::hex::decode(sig_hash).unwrap();
        let sig = xmtp_cryptography::signature::RecoverableSignature::Eip191Signature(sig_bytes);

        let recovered_addr = sig.recover_address(msg).unwrap();
        assert_eq!(recovered_addr, addr.to_lowercase());
    }
}
