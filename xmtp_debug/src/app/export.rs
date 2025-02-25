use crate::{
    app::store::{Database, GroupStore, IdentityStore},
    args,
};

use color_eyre::eyre::{self, bail, eyre, Result};
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, sync::Arc};

use super::types::{Group, Identity};
pub struct Export {
    opts: args::ExportOpts,
    network: args::BackendOpts,
    store: Arc<redb::Database>,
}

impl Export {
    pub fn new(
        opts: args::ExportOpts,
        store: Arc<redb::Database>,
        network: args::BackendOpts,
    ) -> Self {
        Self {
            store,
            opts,
            network,
        }
    }

    pub fn run(self) -> Result<()> {
        use args::EntityKind::*;
        let Export {
            opts,
            network,
            store,
        } = self;
        let args::ExportOpts {
            entity,
            out,
            inbox_id,
        } = opts;
        let mut writer: Box<dyn Write> = if let Some(p) = out {
            Box::new(fs::File::create_new(p)?)
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
            SingleIdentity => {
                let store: IdentityStore = store.into();
                if let Some(mut ids) = store.load(&network)? {
                    let id = if let Some(inbox) = inbox_id {
                        let mut target: [u8; 32] = [0u8; 32];
                        hex::decode_to_slice(&inbox, &mut target)?;
                        let identity = ids
                            .find(|i| i.value().inbox_id == target)
                            .map(|i| IdentityExport::from(i.value()))
                            .ok_or(eyre!("no identity found for inbox_id {inbox}"))?;
                        Ok::<_, eyre::Report>(identity)
                    } else {
                        let identity = ids
                            .choose(&mut rand::thread_rng())
                            .map(|i| IdentityExport::from(i.value()))
                            .ok_or(eyre!("empty identities"))?;
                        Ok(identity)
                    }?;
                    let json = serde_json::to_string(&id)?;
                    writer.write_all(json.as_bytes())?;
                    writer.flush()?;
                } else {
                    bail!("No identities in store");
                };
            }
            Message => todo!(),
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct IdentityExportInfo {
    ethereum_address: String,
    installation_private_key: String,
    ethereum_private_key: String,
}

#[derive(Serialize, Deserialize)]
pub struct IdentityExport {
    inbox_id: String,
    info: IdentityExportInfo,
}

impl From<Identity> for IdentityExport {
    fn from(identity: Identity) -> Self {
        IdentityExport {
            inbox_id: hex::encode(identity.inbox_id),
            info: IdentityExportInfo {
                ethereum_address: identity.address(),
                installation_private_key: hex::encode(identity.installation_key),
                ethereum_private_key: hex::encode(identity.eth_key),
            },
        }
    }
}

impl TryFrom<IdentityExport> for Identity {
    type Error = color_eyre::eyre::Report;
    fn try_from(identity: IdentityExport) -> Result<Self, Self::Error> {
        Ok(Identity {
            inbox_id: hex::decode(identity.inbox_id)?.as_slice().try_into()?,
            installation_key: hex::decode(identity.info.installation_private_key)?
                .as_slice()
                .try_into()?,
            eth_key: hex::decode(identity.info.ethereum_private_key)?
                .as_slice()
                .try_into()?,
        })
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
