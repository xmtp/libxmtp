use crate::{
    app::{
        App,
        store::{Database, GroupStore, IdentityStore},
    },
    args::{self, BackendOpts, ExportOpts},
};

use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, sync::Arc};

use super::types::{Group, Identity};
pub struct Export {
    opts: args::ExportOpts,
    network: args::BackendOpts,
    store: Arc<redb::Database>,
}

impl Export {
    pub fn new(opts: ExportOpts, network: BackendOpts) -> Result<Self> {
        let store = App::db()?;
        Ok(Self {
            store,
            opts,
            network,
        })
    }

    pub fn run(self) -> Result<()> {
        use args::EntityKind::*;
        let Export {
            opts,
            network,
            store,
        } = self;
        let args::ExportOpts { entity, out } = opts;
        let mut writer: Box<dyn Write> = if let Some(p) = out {
            Box::new(fs::File::create(p)?)
        } else {
            Box::new(std::io::stdout())
        };

        match entity {
            Identity => {
                let store: IdentityStore = store.into();
                if let Some(ids) = store.load(&network)? {
                    let ids = ids
                        .map(|i| IdentityExport::from(i.value()))
                        .collect::<Vec<_>>();
                    let json = serde_json::to_string(&ids)?;
                    writer.write_all(json.as_bytes())?;
                    writer.flush()?;
                };
            }
            Group => {
                let store: GroupStore = store.into();
                if let Some(groups) = store.load(&network)? {
                    let groups = groups
                        .map(|g| GroupExport::from(g.value()))
                        .collect::<Vec<_>>();
                    let json = serde_json::to_string(&groups)?;
                    writer.write_all(json.as_bytes())?;
                    writer.flush()?;
                };
            }
            Message => todo!(),
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct IdentityExport {
    inbox_id: String,
    ethereum_address: String,
}

impl From<Identity> for IdentityExport {
    fn from(identity: Identity) -> Self {
        IdentityExport {
            inbox_id: hex::encode(identity.inbox_id),
            ethereum_address: identity.address(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GroupExport {
    id: String,
    group: GroupExportInner,
}

#[derive(Serialize, Deserialize)]
pub struct GroupExportInner {
    created_by: String,
    member_size: u32,
    members: Vec<String>,
}

impl From<Group> for GroupExport {
    fn from(group: Group) -> Self {
        GroupExport {
            id: hex::encode(group.id),
            group: GroupExportInner {
                created_by: hex::encode(group.created_by),
                member_size: group.member_size,
                members: group.members.into_iter().map(hex::encode).collect(),
            },
        }
    }
}
