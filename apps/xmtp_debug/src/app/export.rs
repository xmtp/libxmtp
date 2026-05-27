use crate::{
    app::{
        App,
        store::{Database, GroupStore, IdentityStore},
    },
    args::{self, ExportOpts},
};

use color_eyre::eyre::{OptionExt, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, sync::Arc};
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_proto::types::Topic;

use super::types::{Group, Identity};
pub struct Export {
    opts: &'static args::ExportOpts,
    store: Arc<redb::Database>,
}

impl Export {
    pub fn new(opts: &'static ExportOpts) -> Result<Self> {
        let store = App::db()?;
        Ok(Self { store, opts })
    }

    pub fn run(self) -> Result<()> {
        use args::ExportEntityKind::*;
        let Export { opts, store } = self;
        let args::ExportOpts { entity, out } = opts;
        let mut writer: Box<dyn Write> = if let Some(p) = out {
            Box::new(fs::File::create(p)?)
        } else {
            Box::new(std::io::stdout())
        };

        match entity {
            Identity => {
                let store: IdentityStore = store.into();
                let ids = store.load()?.ok_or_eyre("no ids in store")?;
                let ids = ids
                    .map(|i| IdentityExport::from(i.value()))
                    .collect::<Vec<_>>();
                let json = serde_json::to_string(&ids)?;
                writer.write_all(json.as_bytes())?;
                writer.flush()?;
            }
            Group => {
                let store: GroupStore = store.into();
                let groups = store.load()?.ok_or_eyre("no groups in store")?;
                let groups = groups
                    .map(|g| GroupExport::from(g.value()))
                    .collect::<Vec<_>>();
                let json = serde_json::to_string(&groups)?;
                writer.write_all(json.as_bytes())?;
                writer.flush()?;
            }
            Message => todo!(),
            IdentityTopics => {
                let store: IdentityStore = store.into();
                let ids = store.load()?.ok_or_eyre("no identities in store")?;
                let topics: Vec<Topic> = ids
                    .map(|i| Topic::new_identity_update(i.value().inbox_id))
                    .collect();
                let json = serde_json::to_string(&topics)?;
                writer.write_all(json.as_bytes())?;
                writer.flush()?;
            }
            GroupTopics => {
                let store: GroupStore = store.into();
                let groups = store.load()?.ok_or_eyre("no groups in store")?;
                let topics: Vec<Topic> = groups
                    .map(|g| Topic::new_group_message(g.value().id()))
                    .collect();
                let json = serde_json::to_string(&topics)?;
                writer.write_all(json.as_bytes())?;
                writer.flush()?;
            }
            KeyPackageTopics => {
                let store: IdentityStore = store.into();
                let ids = store.load()?.ok_or_eyre("no identities in store")?;
                let topics: Vec<Topic> = ids
                    .map(|i| Topic::new_key_package(i.value().installation_key))
                    .collect();
                let json = serde_json::to_string(&topics)?;
                writer.write_all(json.as_bytes())?;
                writer.flush()?;
            }
            WelcomeMessageTopics => {
                let store: IdentityStore = store.into();
                let ids = store.load()?.ok_or_eyre("no identities in store")?;
                let topics: Vec<Topic> = ids
                    .map(|i| Topic::new_welcome_message(i.value().installation_key.into()))
                    .collect();
                let json = serde_json::to_string(&topics)?;
                writer.write_all(json.as_bytes())?;
                writer.flush()?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct IdentityExport {
    inbox_id: String,
    ethereum_address: String,
    installation_public_key: String,
    version: String,
}

impl From<Identity> for IdentityExport {
    fn from(identity: Identity) -> Self {
        let inst = XmtpInstallationCredential::from_bytes(&identity.installation_key).unwrap();
        IdentityExport {
            inbox_id: hex::encode(identity.inbox_id),
            ethereum_address: identity.address(),
            installation_public_key: hex::encode(inst.public_bytes()),
            version: String::from_utf8_lossy(&identity.version_string)
                .trim_matches('\0')
                .to_string(),
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
    version: String,
}

impl From<Group> for GroupExport {
    fn from(group: Group) -> Self {
        GroupExport {
            id: hex::encode(group.id()),
            group: GroupExportInner {
                created_by: hex::encode(group.created_by),
                member_size: group.member_size,
                members: group.members.into_iter().map(hex::encode).collect(),
                version: String::from_utf8_lossy(&group.version_string)
                    .trim_matches('\0')
                    .to_string(),
            },
        }
    }
}
