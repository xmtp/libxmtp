use color_eyre::eyre::{bail, Result};
use std::sync::Arc;
use valuable::Valuable;

use crate::{
    app,
    app::store::{Database, IdentityStore},
    args,
};

pub struct Inspect {
    db: Arc<redb::Database>,
    opts: args::Inspect,
    network: args::BackendOpts,
}

impl Inspect {
    pub fn new(opts: args::Inspect, network: args::BackendOpts, db: Arc<redb::Database>) -> Self {
        Self { opts, network, db }
    }

    pub async fn run(self) -> Result<()> {
        use args::InspectionKind::*;
        let Inspect { db, opts, network } = self;

        let identity_store: IdentityStore = db.clone().into();
        // let group_store: GroupStore = db.clone().into();

        let args::Inspect { kind, inbox_id } = opts;
        let key = (u64::from(&network), *inbox_id);
        let identity = identity_store.get(key.into())?;
        if identity.is_none() {
            bail!("No local identity with inbox_id=[{}]", inbox_id);
        }
        let client =
            app::client_from_identity(&identity.expect("checked for none"), &network).await?;
        let conn = client.store().conn()?;
        match kind {
            Associations => {
                let state = client
                    .get_latest_association_state(&conn, &hex::encode(*inbox_id))
                    .await?;
                info!(
                    inbox_id = state.inbox_id(),
                    account_addresses = state.account_addresses().as_value(),
                    installations = state
                        .installation_ids()
                        .into_iter()
                        .map(hex::encode)
                        .collect::<Vec<_>>()
                        .as_value(),
                    recovery_address = state.recovery_address(),
                    "latest association state"
                );
            }
            Groups => {
                info!("Inspecting groups");
                #[derive(Debug, Valuable)]
                struct PrintableGroup {
                    group_id: String,
                    created_at_ns: i64,
                }
                let groups = client
                    .find_groups(Default::default())?
                    .into_iter()
                    .map(|g| PrintableGroup {
                        group_id: hex::encode(g.group_id),
                        created_at_ns: g.created_at_ns,
                    })
                    .collect::<Vec<PrintableGroup>>();
                for group in groups.iter() {
                    info!(group.group_id, group.created_at_ns, "group");
                }
            }
        }
        Ok(())
    }
}
