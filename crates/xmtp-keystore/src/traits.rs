use crate::signature::Signature;

// Trait for whether an associated Wallet Address can be extracted
pub trait WalletAssociated {
    fn wallet_address(&self) -> Result<String, String>;
}

// Trait signature verifiable
pub trait SignatureVerifiable {
    fn get_signature(&self) -> Result<Signature, String>;
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
