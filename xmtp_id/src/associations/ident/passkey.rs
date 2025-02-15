use std::fmt::Display;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct PubPasskey();

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Passkey(pub [u8; Passkey::KEY_SIZE]);

impl Passkey {
    pub const KEY_SIZE: usize = 33;
}

impl Display for Passkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.public_key))
    }
}
