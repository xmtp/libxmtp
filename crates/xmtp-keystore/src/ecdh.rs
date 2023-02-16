use k256::PublicKey;

pub trait ECDHKey {
    fn get_public_key(&self) -> PublicKey;
}

// Diffie-hellman trait for keys
pub trait ECDHDerivable {
    fn shared_secret(&self, other_public_key: &dyn ECDHKey) -> Result<Vec<u8>, String>;
}
