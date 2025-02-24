use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use crate::{app::clients, args};
use color_eyre::eyre::{eyre, Result};
use futures::stream::StreamExt;

use super::{
    export::IdentityExport,
    store::{Database, IdentityStore},
    types::Identity,
};

#[derive(Debug)]
pub struct Stream {
    db: Arc<redb::Database>,
    opts: args::Stream,
    network: args::BackendOpts,
}

impl Stream {
    pub fn new(opts: args::Stream, network: args::BackendOpts, db: Arc<redb::Database>) -> Self {
        Self { opts, network, db }
    }

    pub async fn run(self) -> Result<()> {
        let args::Stream {
            inbox_id,
            ref import,
        } = self.opts;

        let identity: Identity = if let Some(file) = import {
            let mut file = File::open(file)?;
            let mut s = String::new();
            file.read_to_string(&mut s)?;
            let json: IdentityExport = miniserde::json::from_str(&s)?;
            let identity: Identity = json.try_into()?;
            // create a new installation
            let _ =
                clients::new_installation_from_identity(identity.clone(), &self.network).await?;
            assert_eq!(
                identity.inbox_id, *inbox_id,
                "imported inbox id and provided inbox id must match"
            );
            identity
        } else {
            let identity_store: IdentityStore = self.db.clone().into();
            let key = (u64::from(&self.network), *inbox_id);
            let id: Identity = identity_store.get(key.into())?.ok_or(eyre!(
                "No identity matching inbox id {} in store",
                hex::encode(*inbox_id)
            ))?;
            id
        };

        let client = clients::client_from_identity(&identity, &self.network).await?;
        let mut stream = client.stream_all_messages(None).await?;
        let mut stream = std::pin::pin!(stream);
        while let Some(m) = stream.next().await {
            match m {
                Ok(msg) => {
                    println!(
                        "{}",
                        String::from_utf8_lossy(msg.decrypted_message_bytes.as_slice())
                    );
                }
                Err(e) => {
                    error!("{}", e);
                }
            }
        }
        Ok(())
    }
}

// list of refs in nix to build xdbg for
// spit out binaries
// script can loop over binaries
// create identities in different databases
// one stream should receive all messages
