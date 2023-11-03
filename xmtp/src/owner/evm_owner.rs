pub use ethers::signers::{LocalWallet, Signer};
use futures::executor;
use xmtp_cryptography::signature::{h160addr_to_string, RecoverableSignature, SignatureError};

use crate::InboxOwner;

impl InboxOwner for LocalWallet {
    fn get_address(&self) -> String {
        h160addr_to_string(self.address())
    }

    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError> {
        let signature = executor::block_on(self.sign_message(text))
            .map_err(|e| SignatureError::ThirdPartyError(e.to_string()))?;

        Ok(RecoverableSignature::Eip191Signature(signature.to_vec()))
    }
}
