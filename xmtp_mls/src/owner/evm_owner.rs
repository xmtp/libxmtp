pub use ethers::signers::{LocalWallet, Signer};
use futures::executor;

use xmtp_cryptography::signature::{h160addr_to_string, RecoverableSignature, SignatureError};

use crate::InboxOwner;

impl InboxOwner for LocalWallet {
    fn get_address(&self) -> String {
        h160addr_to_string(self.address())
    }

    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError> {
        Ok(executor::block_on(self.sign_message(text))?.to_vec().into())
    }
}
