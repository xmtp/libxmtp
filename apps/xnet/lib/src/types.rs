//! Common Types
use alloy::{primitives::Address, signers::local::PrivateKeySigner};
use bon::Builder;
use color_eyre::eyre::Result;
use xmtp_api_d14n::d14n::GetNodes;
use xmtp_api_grpc::GrpcClient;
use xmtp_proto::{
    api::Query,
    prelude::{ApiBuilder, NetConnectConfig},
};

use crate::{Config, constants::Xmtpd as XmtpdConst, services::allocate_xmtpd_port};

/// An XMTPD node that must run at `port`
/// and is owned by `signer`.
#[derive(Debug, Clone, Builder)]
pub struct XmtpdNode {
    port: u16,
    signer: PrivateKeySigner,
    payer: PrivateKeySigner,
    migration_payer: PrivateKeySigner,
    node_id: u32,
    name: String,
}

/// Determine the port for an XMTPD node.
///
/// Rules:
/// - `use_standard_port=true` + explicit port → error
/// - `use_standard_port=true` + no port       → 5050 (XmtpdConst::GRPC_PORT)
/// - `use_standard_port=false` + explicit port → use that port
/// - `use_standard_port=false` + no port       → allocate from range 8150-8200
pub fn resolve_port(use_standard_port: bool, port: Option<u16>) -> Result<u16> {
    match (use_standard_port, port) {
        (true, Some(_)) => color_eyre::eyre::bail!(
            "cannot set both `use_standard_port` and an explicit `port` for the same node"
        ),
        (true, None) => Ok(XmtpdConst::GRPC_PORT),
        (false, Some(p)) => Ok(p),
        (false, None) => allocate_xmtpd_port(),
    }
}

impl XmtpdNode {
    pub async fn new(gateway_host: &str, use_standard_port: bool) -> Result<Self> {
        let config = Config::load()?;
        let next_id = Self::get_next_id(gateway_host).await?;
        let port = resolve_port(use_standard_port, None)?;

        let num_ids = next_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        Ok(Self {
            port,
            node_id: next_id,
            signer: config.signers[base_idx].clone(),
            payer: config.signers[base_idx + 1].clone(),
            migration_payer: config.signers[base_idx + 2].clone(),
            name: format!("xnet-{}", next_id),
        })
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn id(&self) -> &u32 {
        &self.node_id
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
    }

    pub fn payer(&self) -> &PrivateKeySigner {
        &self.payer
    }

    pub fn migration_payer(&self) -> &PrivateKeySigner {
        &self.migration_payer
    }

    pub fn address(&self) -> Address {
        self.signer.address()
    }

    pub fn payer_address(&self) -> Address {
        self.payer.address()
    }

    pub fn migration_payer_address(&self) -> Address {
        self.migration_payer.address()
    }
    pub fn public_key(&self) -> [u8; 64] {
        *self.signer.public_key()
    }

    /// Returns the SEC1 compressed public key (33 bytes: prefix + x-coordinate).
    pub fn compressed_public_key(&self) -> Vec<u8> {
        use alloy::signers::k256::elliptic_curve::sec1::ToEncodedPoint;
        let pubkey = self.signer.credential().verifying_key();
        pubkey.to_encoded_point(true).as_bytes().to_vec()
    }

    pub async fn get_next_id(gateway_host: &str) -> Result<u32> {
        let mut grpc = GrpcClient::builder();
        grpc.set_host(gateway_host.parse()?);
        let grpc = grpc.build()?;
        let nodes = GetNodes::builder().build()?.query(&grpc).await?;
        let ids = nodes.nodes.keys().max();
        Ok(ids.unwrap_or(&0) + XmtpdConst::NODE_ID_INCREMENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_port_standard_port_none_returns_5050() {
        let result = resolve_port(true, None).unwrap();
        assert_eq!(result, XmtpdConst::GRPC_PORT);
    }

    #[test]
    fn resolve_port_auto_allocates_in_range() {
        use crate::constants::ToxiProxy as ToxiProxyConst;
        let result = resolve_port(false, None).unwrap();
        assert!(
            result >= ToxiProxyConst::XMTPD_PORT_RANGE.0
                && result < ToxiProxyConst::XMTPD_PORT_RANGE.1,
            "allocated port {} not in range {}..{}",
            result,
            ToxiProxyConst::XMTPD_PORT_RANGE.0,
            ToxiProxyConst::XMTPD_PORT_RANGE.1,
        );
    }

    #[test]
    fn resolve_port_explicit_port_returns_that_port() {
        let result = resolve_port(false, Some(9999)).unwrap();
        assert_eq!(result, 9999);
    }

    #[test]
    fn resolve_port_standard_port_and_explicit_errors() {
        let result = resolve_port(true, Some(9999));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("cannot set both"));
    }
}
