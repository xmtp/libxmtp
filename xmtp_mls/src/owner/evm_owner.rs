pub use ethers::signers::{LocalWallet, Signer};

use xmtp_cryptography::signature::{h160addr_to_string, RecoverableSignature, SignatureError};

use crate::InboxOwner;

impl InboxOwner for LocalWallet {
    fn get_address(&self) -> String {
        h160addr_to_string(self.address())
    }

    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError> {
        let message_hash = ethers_core::utils::hash_message(text);
        Ok(self.sign_hash(message_hash)?.to_vec().into())
    }
}
