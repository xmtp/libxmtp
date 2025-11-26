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
        } = opts;
        let key = (u64::from(&network), *group_id);
        let mut local_group = group_store.get(key.into())?.ok_or(eyre!(
            "no local group found for id=[{}]",
            hex::encode(*group_id)
        ))?;
        let key = (u64::from(&network), local_group.created_by);
        let identity = identity_store.get(key.into())?.ok_or(eyre!(
            "no local identity found for inbox_id=[{}]",
            hex::encode(local_group.created_by)
        ))?;
        let admin = app::client_from_identity(&identity, &network)?;
        let group = admin.group(&local_group.id.to_vec())?;
        match action {
            Remove => {
                if inbox_id.is_none() {
                    bail!("Inbox ID to remove must be specificied")
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
                    .ok_or(eyre!("No identitites"))?
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
                    bail!("Inbox ID to add must be specificied")
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
        }

        Ok(())
    }
}
