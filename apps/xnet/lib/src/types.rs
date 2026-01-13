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
    node_id: u32,
    name: String,
}

impl XmtpdNode {
    pub async fn new(gateway_host: &str) -> Result<Self> {
        let config = Config::load()?;
        let next_id = Self::get_next_id(gateway_host).await?;
        // if its the first node use the standard 5050 for compatibility
        let port = if config.use_standard_ports && next_id == XmtpdConst::NODE_ID_INCREMENT {
            5050
        } else {
            allocate_xmtpd_port()?
        };

        let config = Config::load()?;
        let num_ids = next_id / XmtpdConst::NODE_ID_INCREMENT;
        let next_signer = &config.signers[num_ids as usize + 1];
        Ok(Self {
            port,
            node_id: next_id,
            signer: next_signer.clone(),
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

    pub fn address(&self) -> Address {
        self.signer.address()
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
        grpc.set_host(gateway_host.into());
        grpc.set_tls(false);
        let grpc = grpc.build()?;
        let nodes = GetNodes::builder().build()?.query(&grpc).await?;
        let ids = nodes.nodes.keys().max();
        Ok(ids.unwrap_or(&0) + XmtpdConst::NODE_ID_INCREMENT)
    }
}
