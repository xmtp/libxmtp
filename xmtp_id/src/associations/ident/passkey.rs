use serde::{Deserialize, Serialize};
use std::{fmt::Display, hash::Hash};

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct Passkey {
    pub key: Vec<u8>,
    pub relying_partner: Option<String>,
}

impl PartialEq for Passkey {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Hash for Passkey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}
impl Display for Passkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.key))
    }
}
