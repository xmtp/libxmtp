use anyhow::Result;
use std::path::Path;
use xmtp_db::{EncryptedMessageStore, NativeDb, diesel::connection::SimpleConnection};

pub fn db_vacuum(store: &EncryptedMessageStore<NativeDb>, dest: impl AsRef<Path>) -> Result<()> {
    let buff = store.db().raw_query_write(|conn| {
        let buff = conn.serialize_database_to_buffer();
        Ok(buff)
    })?;
    std::fs::write(dest, buff.as_slice())?;

    Ok(())
}
