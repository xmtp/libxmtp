use k256::PublicKey;

use crate::signature::Signature;

// Trait for whether an associated Wallet Address can be extracted
pub trait WalletAssociated {
    fn wallet_address(&self) -> Result<String, String>;
}

// Trait signature verifiable
pub trait SignatureVerifiable {
    fn get_signature(&self) -> Option<Signature>;
}

// Trait for Protobuf serialization / deserialization
// - looked at other options online but for now we can
// start by implementing this simple trait
pub trait Buffable {
    fn to_proto_bytes(&self) -> Result<Vec<u8>, String>;

    fn from_proto_bytes(buff: &[u8]) -> Result<Self, String>
    where
        Self: Sized;
}

pub trait ECDHKey {
    fn get_public_key(&self) -> PublicKey;
}

// Diffie-hellman trait for keys
pub trait ECDHDerivable {
    fn shared_secret(&self, other_public_key: &impl ECDHKey) -> Result<Vec<u8>, String>;
}

// Combination of ECDHKey, ECDHDerivable and SignatureVerifiable
pub trait SignedECDHKey: ECDHKey + SignatureVerifiable + {}

pub trait VerifiableKeyBundle<I,P>: ECDHKey + ECDHDerivable + SignatureVerifiable + WalletAssociated + Buffable + Sized
where
    I: ECDHKey + ECDHDerivable + SignatureVerifiable + WalletAssociated + Buffable + Sized,
    P: ECDHKey + ECDHDerivable + SignatureVerifiable + WalletAssociated + Buffable + Sized,
{
    fn get_identity_key(&self) -> I;
    fn get_prekey(&self) -> P;
}

pub trait BridgeSignableVersion<U, S> {
    fn to_signed(&self) -> S;
    fn to_unsigned(&self) -> U;
}
