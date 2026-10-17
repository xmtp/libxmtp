//! Fresh identities and stable database ownership for one run.
use crate::protocol::InstanceConfig;
use anyhow::{Result, ensure};
use rand::Rng;
use std::path::Path;

pub(crate) const INBOXES: usize = 6;
pub(crate) const PROXY_SLOTS: usize = 19;
pub(crate) const SHARED_SLOT: usize = 6;

pub(crate) fn configs(root: &Path, seed: u64, endpoints: &[String]) -> Result<Vec<InstanceConfig>> {
    ensure!(endpoints.len() == PROXY_SLOTS, "incorrect proxy count");
    // Reusing a schedule seed must not invite abandoned installations from a prior run.
    let mut rng = rand::rng();
    let mut wallets = Vec::new();
    for _ in 0..INBOXES {
        loop {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            let wallet = hex::encode(bytes);
            if wallet
                .parse::<alloy_signer_local::PrivateKeySigner>()
                .is_ok()
            {
                wallets.push(wallet);
                break;
            }
        }
    }
    let mut configs: Vec<InstanceConfig> = Vec::new();
    for (slot, endpoint) in endpoints.iter().enumerate() {
        if slot == SHARED_SLOT {
            let mut shared = configs[0].clone();
            shared.slot = slot;
            shared.endpoint = endpoint.clone();
            shared.stream_owner = false;
            configs.push(shared);
            continue;
        }
        let inbox_index = if slot < INBOXES {
            slot
        } else {
            (slot - 7) % INBOXES
        };
        let mut key = [0u8; 32];
        rng.fill_bytes(&mut key);
        configs.push(InstanceConfig {
            slot,
            inbox_index,
            database: root.join(format!("instance-{slot}.db3")),
            database_key: hex::encode(key),
            wallet_key: wallets[inbox_index].clone(),
            endpoint: endpoint.clone(),
            seed: seed.wrapping_add(slot as u64),
            stream_owner: true,
        });
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn repeated_schedule_seed_uses_fresh_inboxes_and_keeps_shared_database() {
        let endpoints = vec!["http://127.0.0.1:1".into(); PROXY_SLOTS];
        let first = configs(Path::new("first"), 42, &endpoints)?;
        let second = configs(Path::new("second"), 42, &endpoints)?;
        assert_ne!(first[0].wallet_key, second[0].wallet_key);
        assert_ne!(first[0].database_key, second[0].database_key);
        assert_eq!(first[0].wallet_key, first[SHARED_SLOT].wallet_key);
        assert_eq!(first[0].database, first[SHARED_SLOT].database);
        assert_eq!(first[0].database_key, first[SHARED_SLOT].database_key);
        assert_eq!(first[0].seed, second[0].seed);
    }
}
