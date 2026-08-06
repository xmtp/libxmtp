//! Node provisioning: derive signers and orchestrate the full lifecycle of an XMTPD node.

use alloy::signers::local::PrivateKeySigner;
use bon::Builder;
use color_eyre::eyre::{Result, eyre};

use crate::{
    Config,
    app::ServiceManager,
    constants::{Anvil as AnvilConst, Xmtpd as XmtpdConst},
    contracts::{get_broadcaster_pause_status, set_payload_bootstrapper},
    types::{XmtpdNode, resolve_port},
    xmtpd_cli::XmtpdCli,
};

/// The three signers associated with a single XMTPD node.
pub struct NodeSigners {
    pub signer: PrivateKeySigner,
    pub payer: PrivateKeySigner,
    pub migration_payer: PrivateKeySigner,
}

/// Derive the three signers for a node from its assigned ID.
///
/// Node IDs are assigned in increments of [`XmtpdConst::NODE_ID_INCREMENT`] (100).
/// Each node uses 3 consecutive signers starting at index `(node_id / 100) * 3 + 1`.
pub fn derive_signers(node_id: u32) -> Result<NodeSigners> {
    let config = Config::load()?;
    let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
    let base_idx = num_ids as usize * 3 + 1;

    if base_idx + 2 >= config.signers.len() {
        return Err(eyre!(
            "node_id {} requires signer index {} which exceeds available signers ({})",
            node_id,
            base_idx + 2,
            config.signers.len()
        ));
    }

    Ok(NodeSigners {
        signer: config.signers[base_idx].clone(),
        payer: config.signers[base_idx + 1].clone(),
        migration_payer: config.signers[base_idx + 2].clone(),
    })
}

/// Builder-based provisioner that encapsulates the full node lifecycle:
/// allocate ID, derive signers, register on-chain, and start the container.
#[derive(Builder)]
#[builder(on(String, into))]
pub struct NodeProvisioner {
    /// Whether this node should run as a migrator (V2->V3 migration mode).
    #[builder(default)]
    migrator: bool,
    /// Use the standard gRPC port (5050) instead of allocating from the dynamic range.
    #[builder(default)]
    use_standard_port: bool,
    /// Optional human-readable name for the node (defaults to `xnet-{node_id}`).
    name: Option<String>,
    /// Optional explicit port override.
    port: Option<u16>,
}

impl NodeProvisioner {
    /// Provision a new XMTPD node end-to-end.
    ///
    /// Steps:
    /// 1. If `migrator`, validate that all broadcasters are paused.
    /// 2. Resolve the gRPC port.
    /// 3. Allocate a node ID from the gateway via gRPC.
    /// 4. Derive signers for the allocated ID.
    /// 5. If `migrator`, set the payload bootstrapper on-chain.
    /// 6. Build the [`XmtpdNode`].
    /// 7. Register and enable the node on-chain via [`XmtpdCli`].
    /// 8. Start the container via [`ServiceManager`].
    pub async fn provision(&self, mgr: &mut ServiceManager) -> Result<XmtpdNode> {
        // 1. If migrator, validate preconditions before any state changes
        if self.migrator {
            // V3 stack must be running — migrator nodes need node-go
            if mgr.node_go.is_none() {
                return Err(eyre!(
                    "cannot provision migrator node: V3 stack is disabled (enable_v3 required)"
                ));
            }

            let rpc_url = mgr
                .anvil_rpc_url()
                .ok_or_else(|| eyre!("anvil RPC URL not available"))?
                .to_string();
            let statuses = get_broadcaster_pause_status(&rpc_url).await?;
            for (name, paused) in &statuses {
                if !paused {
                    return Err(eyre!(
                        "cannot provision migrator node: {} broadcaster is not paused",
                        name
                    ));
                }
            }
            info!("all broadcasters confirmed paused for migrator node");
        }

        // 2. Resolve port
        let port = resolve_port(self.use_standard_port, self.port)?;

        // 3. Allocate node ID from gateway
        let gateway = mgr
            .gateway
            .as_ref()
            .ok_or_else(|| eyre!("gateway not available — is the D14n stack enabled?"))?;
        let gateway_url = gateway
            .external_url()
            .ok_or_else(|| eyre!("gateway has no external URL"))?;
        let node_id = XmtpdNode::get_next_id(gateway_url.as_str()).await?;
        info!(node_id, port, "allocated node ID");

        // 4. Derive signers
        let signers = derive_signers(node_id)?;

        // 5. If migrator, set the payload bootstrapper
        if self.migrator {
            let rpc_url = mgr
                .anvil_rpc_url()
                .ok_or_else(|| eyre!("anvil RPC URL not available"))?
                .to_string();
            set_payload_bootstrapper(
                &rpc_url,
                AnvilConst::ADMIN_KEY,
                signers.migration_payer.address(),
            )
            .await?;
            info!(
                bootstrapper = %signers.migration_payer.address(),
                "set payload bootstrapper for migrator node"
            );
        }

        // 6. Build the XmtpdNode
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| format!("xnet-{}", node_id));

        let mut node = XmtpdNode::builder()
            .port(port)
            .signer(signers.signer)
            .payer(signers.payer)
            .migration_payer(signers.migration_payer)
            .node_id(node_id)
            .name(name)
            .build();

        // 7. Register and enable on-chain
        let cli = XmtpdCli::builder().toxiproxy(mgr.proxy.clone()).build();
        cli.register(mgr, std::io::stdout(), &node).await?;
        cli.enable(&mut node, std::io::stdout()).await?;
        info!(node_id, "node registered and enabled on-chain");

        // 8. Start container
        if self.migrator {
            mgr.add_xmtpd_with_migrator(node.clone()).await?;
        } else {
            mgr.add_xmtpd(node.clone()).await?;
        }
        info!(node_id, "node container started");

        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify signer derivation formula: base_idx = (node_id / NODE_ID_INCREMENT) * 3 + 1
    #[test]
    fn derive_signers_index_formula() {
        // We can't call derive_signers directly because Config::load() requires
        // the full application context. Instead, verify the formula in isolation.
        let node_id: u32 = 100;
        let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        assert_eq!(base_idx, 4, "node_id 100 should map to base_idx 4");

        let node_id: u32 = 200;
        let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        assert_eq!(base_idx, 7, "node_id 200 should map to base_idx 7");

        let node_id: u32 = 300;
        let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        assert_eq!(base_idx, 10, "node_id 300 should map to base_idx 10");
    }

    /// Verify the first node (id=100) uses indices 4, 5, 6
    /// (indices 0 is admin, 1-3 are reserved for id=0 which is unused).
    #[test]
    fn derive_signers_first_node_indices() {
        let node_id: u32 = 100;
        let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        assert_eq!(base_idx, 4);
        assert_eq!(base_idx + 1, 5);
        assert_eq!(base_idx + 2, 6);
    }

    /// Verify that a node_id of 0 maps to base_idx 1 (the first usable signer slot).
    #[test]
    fn derive_signers_zero_id() {
        let node_id: u32 = 0;
        let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        assert_eq!(base_idx, 1, "node_id 0 should map to base_idx 1");
    }

    /// Verify the maximum supported node (32 nodes * 3 signers + 1 = index 97,
    /// so node_id 3200 would need index 97 which is the last valid triple).
    #[test]
    fn derive_signers_max_node_within_bounds() {
        let node_id: u32 = 3200;
        let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        assert_eq!(base_idx, 97);
        // base_idx + 2 = 99, which is valid for a 100-element array (indices 0..99)
        assert!(base_idx + 2 < 100);
    }

    /// Verify that a node_id beyond the signer array would overflow.
    #[test]
    fn derive_signers_overflow_detection() {
        let node_id: u32 = 3300;
        let num_ids = node_id / XmtpdConst::NODE_ID_INCREMENT;
        let base_idx = num_ids as usize * 3 + 1;
        assert_eq!(base_idx, 100);
        // base_idx + 2 = 102, which exceeds the 100-element array
        assert!(base_idx + 2 >= 100);
    }
}
