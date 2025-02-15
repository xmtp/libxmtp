use std::fmt::Display;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Ethereum(pub String);

impl Ethereum {
    #[cfg(test)]
    pub fn rand() -> Self {
        Self(xmtp_common::rand_hexstring())
    }

    pub fn sanitize(self) -> Self {
        let addr = self.0.to_lowercase();
        Self(addr)
    }
}

impl Display for Ethereum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
