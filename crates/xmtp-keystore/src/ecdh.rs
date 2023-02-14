use k256::ecdh::SharedSecret;
use k256::PublicKey;

pub trait ECDHKey {
    fn get_public_key(&self) -> PublicKey;
}

// Diffie-hellman trait for keys
pub trait ECDHDerivable {
    fn get_shared_secret(&self, other_public_key: &dyn ECDHKey) -> Result<SharedSecret, String>;
}
