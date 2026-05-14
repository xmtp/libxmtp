use color_eyre::eyre::{Context, Result, bail, eyre};
use rand::{SeedableRng as _, rngs::SmallRng, seq::IteratorRandom};
use std::sync::Arc;
use xmtp_mls::groups::UpdateAdminListType;

use crate::{
    app::{
        self, App,
        store::{Database, GroupStore, IdentityStore},
    },
    args,
};

pub struct Modify {
    db: Arc<redb::Database>,
    opts: args::Modify,
    network: args::BackendOpts,
}

impl Modify {
    pub fn new(opts: args::Modify, network: args::BackendOpts) -> Result<Self> {
        let db = App::db()?;
        Ok(Self { opts, network, db })
    }

    pub async fn run(self) -> Result<()> {
        use args::MemberModificationKind::*;
        let Modify { db, opts, network } = self;

        let identity_store: IdentityStore = db.clone().into();
        let group_store: GroupStore = db.clone().into();
        let args::Modify {
            action,
            group_id,
            inbox_id,
            include_versions,
            promote_super_admin,
        } = opts;
        let key = (u64::from(&network), *group_id);
        let mut local_group = group_store.get(key.into())?.ok_or(eyre!(
            "no local group found for id=[{}]",
            hex::encode(*group_id)
        ))?;

        // `AddFromRedb` manages its own actor selection; skip the shared setup.
        if matches!(action, AddFromRedb) {
            add_members_from_redb(
                &local_group,
                &network,
                &db,
                include_versions,
                promote_super_admin,
            )
            .await?;
            return Ok(());
        }

        let identity = identity_store
            .find_by_inbox(u64::from(&network), local_group.created_by)?
            .ok_or(eyre!(
                "no local identity found for inbox_id=[{}]",
                hex::encode(local_group.created_by)
            ))?;
        let admin = app::client_from_identity(&identity, &network)?;
        let group = admin.group(&local_group.id.to_vec())?;
        match action {
            Remove => {
                if inbox_id.is_none() {
                    bail!("Inbox ID to remove must be specified")
                }
                let inbox_id = inbox_id.expect("Checked for none");
                local_group.member_size -= 1;
                local_group.members.retain(|m| *m != *inbox_id);
                group
                    .remove_members_by_inbox_id(&[&hex::encode(*inbox_id)])
                    .await?;
                // make sure the locally stored group is up to date
                group_store.set(local_group, &network)?;
                info!(
                    removed_inbox_id = hex::encode(*inbox_id),
                    admin_inbox_id = admin.inbox_id(),
                    "member removed"
                );
            }
            AddRandom => {
                let members = &local_group.members;
                let rng = &mut SmallRng::from_entropy();
                let identity = identity_store
                    .load(&network)?
                    .ok_or(eyre!("No identities"))?
                    .map(|i| i.value())
                    .filter(|identity| members.contains(&identity.inbox_id))
                    .choose(rng)
                    .ok_or(eyre!("Identity not found"))?;
                local_group.member_size -= 1;
                local_group.members.push(identity.inbox_id);
                group
                    .add_members_by_inbox_id(&[hex::encode(identity.inbox_id)])
                    .await?;
                info!(
                    inbox_id = hex::encode(identity.inbox_id),
                    group_id = hex::encode(local_group.id),
                    "Member added"
                );
                group_store.set(local_group, &network)?;
            }
            AddExternal => {
                let Some(inbox_id) = inbox_id else {
                    bail!("Inbox ID to add must be specified")
                };
                group
                    .add_members_by_inbox_id(&[hex::encode(*inbox_id)])
                    .await
                    .context("the identity/inbox_id might not exist for this network in the local database")?;
                group
                    .update_admin_list(UpdateAdminListType::AddSuper, inbox_id.to_string())
                    .await?;
                info!(
                    inbox_id = hex::encode(*inbox_id),
                    group_id = hex::encode(local_group.id),
                    added_by = hex::encode(identity.inbox_id),
                    "Member added as Super Admin"
                );
            }
            // AddFromRedb is handled above before this match.
            AddFromRedb => unreachable!("AddFromRedb is dispatched before the shared actor setup"),
        }

        Ok(())
    }
}

async fn add_members_from_redb(
    local_group: &crate::app::types::Group,
    network: &args::BackendOpts,
    db: &Arc<redb::Database>,
    include_versions: args::IncludeVersions,
    promote_super_admin: bool,
) -> Result<()> {
    use args::IncludeVersions;

    let id_store: IdentityStore<'static> = db.clone().into();
    let net_key = u64::from(network);
    let current_vh = App::current_version_hash();

    // Load all identities for the network, then filter by version.
    let candidates: Vec<crate::app::types::Identity> = id_store
        .load(net_key)?
        .ok_or(eyre!("no identities in store"))?
        .map(|guard| guard.value())
        .filter(|id| match include_versions {
            IncludeVersions::Self_ => id.version_hash == current_vh,
            IncludeVersions::Other => id.version_hash != current_vh,
            IncludeVersions::All => true,
        })
        .collect();

    if candidates.is_empty() {
        bail!(
            "no identities match --include-versions={:?}",
            include_versions
        );
    }

    // Pick a same-version actor that's already a group member (and
    // therefore already a super-admin — phase 3 of cross-talk-test
    // promotes joiners to super-admin when adding them).
    let actor_identity = local_group
        .members
        .iter()
        .find_map(|inbox| {
            id_store
                .find_by_inbox(net_key, *inbox)
                .ok()
                .flatten()
                .filter(|id| id.version_hash == current_vh)
        })
        .ok_or_else(|| {
            eyre!(
                "no same-version actor identity found among group members \
                 (group={}, current_version_hash={:016x})",
                hex::encode(local_group.id),
                current_vh
            )
        })?;
    let actor_client = app::client_from_identity(&actor_identity, network)?;
    let group = actor_client.group(&local_group.id.to_vec())?;

    let inbox_ids: Vec<String> = candidates
        .iter()
        .map(|id| hex::encode(id.inbox_id))
        .collect();

    group.add_members_by_inbox_id(&inbox_ids).await?;

    // Persist updated membership to redb so subsequent
    // `modify add-from-redb` calls see this version's new identities.
    {
        let mut updated = local_group.clone();
        let mut seen: std::collections::HashSet<[u8; 32]> =
            updated.members.iter().copied().collect();
        for c in &candidates {
            if seen.insert(c.inbox_id) {
                updated.members.push(c.inbox_id);
            }
        }
        updated.member_size = updated.members.len() as u32;
        let group_store: GroupStore<'static> = db.clone().into();
        group_store.set(updated, u64::from(network))?;
    }

    if promote_super_admin {
        for inbox in &inbox_ids {
            if let Err(e) = group
                .update_admin_list(UpdateAdminListType::AddSuper, inbox.clone())
                .await
            {
                tracing::warn!(
                    target: "xdbg.modify",
                    inbox = %inbox,
                    error = %e,
                    "failed to promote inbox to super-admin"
                );
            }
        }
    }

    tracing::info!(
        target: "xdbg.modify",
        added = inbox_ids.len(),
        promoted = promote_super_admin,
        group = %hex::encode(local_group.id),
        "add-from-redb completed"
    );

    Ok(())
}
