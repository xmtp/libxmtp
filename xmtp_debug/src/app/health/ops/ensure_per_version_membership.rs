//! Strict-versioning-only op: ensure every xdbg version known to this
//! run is represented by at least one member in the newly-created
//! group. If a version is missing, pick one of its inboxes and add
//! it; only fail if a representative can't be found or the add call
//! itself errors.
//!
//! Motivation: cross-talk tests want every version-partition to have
//! at least one member in the test group so downstream ops can drive
//! cross-version traffic. Without `--strict-versioning`, version
//! partitioning isn't meaningful (all identities are loaded
//! uniformly), so this op is gated on `Conditions::STRICT_VERSIONING`.

use crate::app::App;
use crate::app::health::context::{HealthContext, inbox_id_to_bytes};
use crate::app::health::ops::HealthOp;
use crate::app::health::result::{OpResult, Status};
use crate::app::store::{Database, IdentityStore};
use crate::app::types::{Identity, InboxId};
use async_trait::async_trait;
use color_eyre::eyre::eyre;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct EnsurePerVersionMembership;

#[async_trait]
impl HealthOp for EnsurePerVersionMembership {
    fn name(&self) -> &'static str {
        "EnsurePerVersionMembership"
    }

    #[tracing::instrument(
        target = "healthcheck.op",
        skip_all,
        fields(op = "EnsurePerVersionMembership")
    )]
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult> {
        let Some(new_group_id) = ctx.new_groups.first().cloned() else {
            return vec![OpResult {
                op_name: self.name(),
                target: None,
                status: Status::Fail,
                duration: Duration::ZERO,
                error: Some(eyre!(
                    "no new group; CreateGroup + AddMembersToNewGroup must run first"
                )),
            }];
        };

        let start = Instant::now();
        let outcome: color_eyre::eyre::Result<()> = async {
            let id_bytes = <[u8; 16]>::try_from(new_group_id.as_slice())
                .map_err(|_| eyre!("group_id is not 16 bytes"))?;
            let mut updated_members = ctx.persisted_members(id_bytes);
            if updated_members.is_empty() {
                return Err(eyre!(
                    "group has no persisted members; AddMembersToNewGroup must run first"
                ));
            }

            let redb: Arc<redb::Database> = App::db()?;
            let id_store: IdentityStore<'static> = redb.into();
            let net_key = u64::from(&ctx.network);

            let identities: HashMap<InboxId, Identity> = id_store
                .load(net_key)?
                .map(|iter| {
                    iter.map(|g| {
                        let id = g.value();
                        (id.inbox_id, id)
                    })
                    .collect()
                })
                .unwrap_or_default();

            let present_versions: BTreeSet<u64> = updated_members
                .iter()
                .filter_map(|inbox| identities.get(inbox).map(|id| id.version_hash))
                .collect();

            let mut version_representative: BTreeMap<u64, InboxId> = BTreeMap::new();
            version_representative.insert(
                App::current_version_hash(),
                inbox_id_to_bytes(ctx.primary.inbox_id()),
            );
            for inbox in &ctx.other_identities {
                if let Some(id) = identities.get(inbox) {
                    version_representative
                        .entry(id.version_hash)
                        .or_insert(*inbox);
                }
            }

            let to_add: Vec<InboxId> = version_representative
                .iter()
                .filter(|(v, _)| !present_versions.contains(v))
                .map(|(_, inbox)| *inbox)
                .collect();

            if to_add.is_empty() {
                return Ok(());
            }

            let hex_to_add: Vec<String> = to_add.iter().map(hex::encode).collect();

            tracing::info!(
                target: "healthcheck",
                group = %new_group_id,
                adding = ?hex_to_add,
                "adding representative members for missing versions",
            );

            let group = ctx
                .primary
                .group(&new_group_id.to_vec())
                .map_err(|e| eyre!("primary cannot load group: {e}"))?;
            group
                .add_members_by_inbox_id(&hex_to_add)
                .await
                .map_err(|e| eyre!("{e}"))?;

            updated_members.extend(to_add);
            ctx.update_group_members(id_bytes, updated_members);

            // Welcomes aren't auto-pulled mid-run — sync so the new
            // members can `client.group(...)` immediately.
            ctx.sync_welcomes_fanout(self.name()).await;

            Ok(())
        }
        .await;

        let (status, error) = match outcome {
            Ok(_) => (Status::Pass, None),
            Err(e) => (Status::Fail, Some(e)),
        };

        vec![OpResult {
            op_name: self.name(),
            target: Some(format!("{new_group_id}")),
            status,
            duration: start.elapsed(),
            error,
        }]
    }
}

inventory::submit! {
    crate::app::health::ops::OpEntry {
        op_name: "EnsurePerVersionMembership",
        depends_on: &["AddMembersToNewGroup"],
        make: || Box::new(EnsurePerVersionMembership),
        requires: crate::app::health::conditions::Conditions::STRICT_VERSIONING,
    }
}
