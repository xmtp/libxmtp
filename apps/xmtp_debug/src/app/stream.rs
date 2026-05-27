use color_eyre::eyre::{Result, bail, eyre};
use futures::stream::StreamExt;
use rand::{SeedableRng, rngs::SmallRng, seq::IteratorRandom};
use serde::Serialize;
use std::{
    fs,
    io::{BufWriter, Write},
    sync::Arc,
};
use xmtp_db::group::StoredGroup;

use crate::{
    app::{
        self, App,
        store::{Database, IdentityStore},
    },
    args::{self, FormatKind, StreamOpts},
};

pub struct Stream {
    db: Arc<redb::ReadOnlyDatabase>,
    opts: &'static args::StreamOpts,
}

impl Stream {
    pub fn new(opts: &'static StreamOpts) -> Result<Self> {
        let db = App::readonly_db()?;
        Ok(Self { opts, db })
    }

    pub async fn run(self) -> Result<()> {
        let Stream { db, opts } = self;
        let identity_store: IdentityStore = db.clone().into();
        let args::StreamOpts {
            inbox,
            kind,
            out,
            format,
        } = opts;
        let rng = &mut SmallRng::from_rng(&mut rand::rng());
        let identity = if let Some(inbox_id) = inbox {
            let identity = identity_store.find_by_inbox(**inbox_id)?;
            if identity.is_none() {
                bail!("No local identity with inbox_id=[{}]", inbox_id);
            }
            identity.expect("checked for some")
        } else {
            identity_store
                .load()?
                .ok_or(eyre!("No identities in store"))?
                .map(|i| i.value())
                .choose(rng)
                .ok_or(eyre!("Identity not found"))?
        };
        let client = app::client_from_identity(&identity)?;

        let mut buffer: Box<dyn Write> = if let Some(p) = out {
            Box::new(BufWriter::new(fs::File::create(p)?))
        } else {
            Box::new(BufWriter::new(std::io::stdout()))
        };

        let fail_fast = crate::fail_fast();
        match kind {
            args::StreamKind::Conversations => {
                let s = client.stream_conversations(None, false).await?;
                tokio::pin!(s);
                while let Some(item) = s.as_mut().next().await {
                    let group = match item {
                        Ok(g) => g,
                        Err(e) if fail_fast => return Err(e.into()),
                        Err(e) => {
                            tracing::warn!(error = %e, "stream_conversations item error");
                            continue;
                        }
                    };
                    let stored: StoredGroup = group.load()?;
                    let item = Conversation {
                        group_id: hex::encode(stored.id),
                        dm_id: stored.dm_id.unwrap_or("".to_string()),
                        conversation_type: stored.conversation_type as i32,
                        created_at: stored.created_at_ns,
                        maybe_forked: stored.maybe_forked,
                        fork_details: stored.fork_details,
                        sequence_id: stored.sequence_id.unwrap_or(0),
                        originator_id: stored.originator_id.unwrap_or(0),
                        added_by: stored.added_by_inbox_id,
                        group_name: group.group_name()?,
                        group_description: group.group_description()?,
                    };
                    write(format, buffer.as_mut(), &item)?;
                    buffer.flush()?;
                }
            }
            args::StreamKind::Messages => {
                let s = client.stream_all_messages(None, None).await?;
                tokio::pin!(s);
                while let Some(next) = s.as_mut().next().await {
                    let message = match next {
                        Ok(m) => m,
                        Err(e) if fail_fast => return Err(e.into()),
                        Err(e) => {
                            tracing::warn!(error = %e, "stream_all_messages item error");
                            continue;
                        }
                    };
                    let item = Message {
                        contents: String::from_utf8_lossy(&message.decrypted_message_bytes)
                            .to_string(),
                        sender: message.sender_inbox_id,
                        receiver: hex::encode(identity.inbox_id),
                        timestamp: message.sent_at_ns,
                        sequence_id: message.sequence_id,
                        originator_id: message.originator_id,
                        group_id: hex::encode(message.group_id),
                        expire_at: message.expire_at_ns.unwrap_or(0),
                        content_type: message.content_type as i32,
                        version_major: message.version_major,
                        version_minor: message.version_minor,
                        authority_id: message.authority_id,
                    };
                    write(format, buffer.as_mut(), &item)?;
                    buffer.flush()?;
                }
            }
        };
        Ok(())
    }
}

fn write(format: &FormatKind, writer: &mut dyn Write, s: &impl Serialize) -> Result<()> {
    use FormatKind::*;
    match format {
        Json => {
            serde_json::to_writer(writer, s)?;
        }
        Csv => {
            let mut csv_writer = csv::Writer::from_writer(writer);
            csv_writer.serialize(s)?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Conversation {
    group_id: String,
    dm_id: String,
    conversation_type: i32,
    created_at: i64,
    maybe_forked: bool,
    fork_details: String,
    sequence_id: i64,
    originator_id: i64,
    added_by: String,
    group_name: String,
    group_description: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Message {
    contents: String,
    sender: String,
    receiver: String,
    timestamp: i64,
    sequence_id: i64,
    originator_id: i64,
    group_id: String,
    expire_at: i64,
    content_type: i32,
    version_major: i32,
    version_minor: i32,
    authority_id: String,
}
