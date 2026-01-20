use crate::{
    SigningError,
    identity::FfiIdentifier,
    inbox_owner::{FfiInboxOwner, IdentityValidationError},
};
use alloy::signers::local::PrivateKeySigner;
use xmtp_id::{
    InboxOwner,
    associations::{test_utils::WalletTestExt, unverified::UnverifiedSignature},
};

#[derive(Clone)]
pub struct FfiWalletInboxOwner {
    pub wallet: PrivateKeySigner,
}

impl Default for FfiWalletInboxOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl FfiWalletInboxOwner {
    pub fn with_wallet(wallet: PrivateKeySigner) -> Self {
        Self { wallet }
    }

    pub fn identifier(&self) -> FfiIdentifier {
        self.wallet.identifier().into()
    }

    pub fn new() -> Self {
        Self {
            wallet: PrivateKeySigner::random(),
        }
    }
}

impl FfiInboxOwner for FfiWalletInboxOwner {
    fn get_identifier(&self) -> Result<FfiIdentifier, IdentityValidationError> {
        let ident = self
            .wallet
            .get_identifier()
            .map_err(|err| IdentityValidationError::Generic(err.to_string()))?;
        Ok(ident.into())
    }

    fn sign(&self, text: String) -> Result<Vec<u8>, SigningError> {
        let recoverable_signature = self.wallet.sign(&text).map_err(|_| SigningError::Generic)?;

        let bytes = match recoverable_signature {
            UnverifiedSignature::RecoverableEcdsa(sig) => sig.signature_bytes().to_vec(),
            _ => unreachable!("Eth wallets only provide ecdsa signatures"),
        };
        Ok(bytes)
    }
}
