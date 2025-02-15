use std::fmt::Display;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Installation(pub Vec<u8>);

impl Display for Installation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}
