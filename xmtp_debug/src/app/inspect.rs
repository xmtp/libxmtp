use color_eyre::eyre::{Result, bail};
use std::sync::Arc;
use valuable::Valuable;

use crate::{
    app::{
        self, App,
        store::{Database, IdentityStore},
    },
    args,
};

pub struct Inspect {
    db: Arc<redb::ReadOnlyDatabase>,
    opts: args::Inspect,
    network: args::BackendOpts,
}

impl Inspect {
    pub fn new(opts: args::Inspect, network: args::BackendOpts) -> Result<Self> {
        let db = App::readonly_db()?;
        Ok(Self { opts, network, db })
    }

    pub async fn run(self) -> Result<()> {
        use args::InspectionKind::*;
        let Inspect { db, opts, network } = self;

        let identity_store: IdentityStore = db.clone().into();

        let args::Inspect { kind, inbox_id } = opts;
        let key = (u64::from(&network), *inbox_id);
        let identity = identity_store.get(key.into())?;
        if identity.is_none() {
            bail!("No local identity with inbox_id=[{}]", inbox_id);
        }
        let client = app::client_from_identity(&identity.expect("checked for none"), &network)?;
        let conn = client.context.store().db();
        match kind {
            Associations => {
                let state = client
                    .identity_updates()
                    .get_latest_association_state(&conn, &hex::encode(*inbox_id))
                    .await?;

                let idents: Vec<_> = state
                    .identifiers()
                    .iter()
                    .map(|ident| format!("{ident:?}"))
                    .collect();
                info!(
                    inbox_id = state.inbox_id(),
                    account_addresses = idents.as_value(),
                    installations = state
                        .installation_ids()
                        .into_iter()
                        .map(hex::encode)
                        .collect::<Vec<_>>()
                        .as_value(),
                    recovery_address = format!("{:?}", state.recovery_identifier()),
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
