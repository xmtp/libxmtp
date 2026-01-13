//! Common Types
use alloy::{primitives::Address, signers::local::PrivateKeySigner};

/// An XMTPD node that must run at `port`
/// and is owned by `signer`.
pub struct XmtpdNode {
    port: u16,
    signer: PrivateKeySigner,
}

impl XmtpdNode {
    pub fn new(port: u16, signer: PrivateKeySigner) -> Self {
        Self { port, signer }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn address(&self) -> Address {
        self.signer.address()
    }
}
