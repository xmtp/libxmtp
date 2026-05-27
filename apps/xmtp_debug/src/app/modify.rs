use color_eyre::eyre::{Context, Result, bail, eyre};
use rand::{SeedableRng, rngs::SmallRng, seq::IteratorRandom};
use std::sync::Arc;
use xmtp_mls::groups::UpdateAdminListType;

use crate::{
    app::{
        self, App,
        store::{Database, GroupStore, IdentityStore},
        types::{Group, Identity},
    },
    args,
};

pub struct Modify {
    db: Arc<redb::Database>,
    opts: &'static args::Modify,
}

impl Modify {
    pub fn new(opts: &'static args::Modify) -> Result<Self> {
        let db = App::db()?;
        Ok(Self { opts, db })
    }

    pub async fn run(self) -> Result<()> {
        use args::MemberModificationKind::*;
        let Modify { db, opts } = self;

        let identity_store: IdentityStore = db.clone().into();
        let group_store: GroupStore = db.clone().into();
        let args::Modify {
            action,
            group_id,
            inbox_id,
            include_versions,
            promote_super_admin,
        } = opts;
        let mut local_group = group_store
            .get((*group_id).into())?
            .ok_or_else(|| eyre!("no local group found for id=[{}]", hex::encode(*group_id)))?;

        // `AddFromRedb` manages its own actor selection; skip the shared setup.
        if matches!(action, AddFromRedb) {
            add_members_from_redb(&local_group, &db, *include_versions, *promote_super_admin)
                .await?;
            return Ok(());
        }

        let identity = identity_store
            .find_by_inbox(local_group.created_by)?
            .ok_or_else(|| {
                eyre!(
                    "no local identity found for inbox_id=[{}]",
                    hex::encode(local_group.created_by)
                )
            })?;
        let admin = app::client_from_identity(&identity)?;
        let group = admin.group(&local_group.id())?;
        match action {
            Remove => {
                let Some(inbox_id) = inbox_id else {
                    bail!("Inbox ID to remove must be specified")
                };
                local_group.member_size -= 1;
                local_group.members.retain(|m| *m != **inbox_id);
                group.remove_members(&[&inbox_id.to_string()]).await?;
                group_store.set(local_group)?;
                info!(
                    removed_inbox_id = %inbox_id,
                    admin_inbox_id = admin.inbox_id(),
                    "member removed"
                );
            }
            AddRandom => {
                let rng = &mut SmallRng::from_rng(&mut rand::rng());
                // Pick an identity NOT already in the group (we're adding a new member).
                let identity = identity_store
                    .load()?
                    .ok_or_else(|| eyre!("no identities in store"))?
                    .map(|i| i.value())
                    .filter(|identity| !local_group.has_member(&identity.inbox_id))
                    .choose(rng)
                    .ok_or_else(|| eyre!("no eligible identity to add"))?;
                local_group.member_size += 1;
                local_group.members.push(identity.inbox_id);
                group.add_members(&[hex::encode(identity.inbox_id)]).await?;
                info!(
                    inbox_id = hex::encode(identity.inbox_id),
                    group_id = %local_group.id(),
                    "Member added"
                );
                group_store.set(local_group)?;
            }
            AddExternal => {
                let Some(inbox_id) = inbox_id else {
                    bail!("Inbox ID to add must be specified")
                };
                group.add_members(&[inbox_id.to_string()]).await.context(
                    "the identity/inbox_id might not exist for this network in the local database",
                )?;
                group
                    .update_admin_list(UpdateAdminListType::AddSuper, inbox_id.to_string())
                    .await?;
                if !local_group.has_member(inbox_id) {
                    local_group.member_size += 1;
                    local_group.members.push(**inbox_id);
                }
                info!(
                    inbox_id = %inbox_id,
                    group_id = %local_group.id(),
                    added_by = hex::encode(identity.inbox_id),
                    "Member added as Super Admin"
                );
                group_store.set(local_group)?;
            }
            AddFromRedb => unreachable!("AddFromRedb is dispatched before the shared actor setup"),
        }

        Ok(())
    }
}

async fn add_members_from_redb(
    local_group: &Group,
    db: &Arc<redb::Database>,
    include_versions: args::IncludeVersions,
    promote_super_admin: bool,
) -> Result<()> {
    use args::IncludeVersions;

    let id_store: IdentityStore<'static> = db.clone().into();
    let current_vh = App::current_version_hash();

    let candidates: Vec<Identity> = id_store
        .load()?
        .ok_or_else(|| eyre!("no identities in store"))?
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

    // Pick a same-version actor among current members. Avoids the
    // false-negative on a joiner's invocation where `created_by`
    // isn't current-version (cross-talk-test phase 5).
    let actor_identity = local_group
        .members
        .iter()
        .find_map(|inbox| {
            id_store
                .find_by_inbox(*inbox)
                .ok()
                .flatten()
                .filter(|id| id.version_hash == current_vh)
        })
        .ok_or_else(|| {
            eyre!(
                "no same-version actor identity found among group members \
                 (group={}, current_version_hash={:016x})",
                local_group.id(),
                current_vh
            )
        })?;
    let actor_client = app::client_from_identity(&actor_identity)?;
    let group = actor_client.group(&local_group.id())?;

    let inbox_ids: Vec<String> = candidates
        .iter()
        .map(|id| hex::encode(id.inbox_id))
        .collect();

    group.add_members(&inbox_ids).await?;

    // Persist membership so subsequent `add-from-redb` invocations
    // (cross-talk-test phase 5) see this version's identities when
    // picking an actor.
    let group_store: GroupStore<'static> = db.clone().into();
    group_store.set(
        local_group
            .clone()
            .add_members(candidates.iter().map(|c| c.inbox_id)),
    )?;

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
        group = %hex::encode(local_group.id()),
        "add-from-redb completed"
    );

    Ok(())
}
