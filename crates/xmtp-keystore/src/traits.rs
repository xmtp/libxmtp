// Trait for whether an associated Wallet Address can be extracted
pub trait WalletAssociated {
    fn wallet_address(&self) -> Result<String, String>;
}
