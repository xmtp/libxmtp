use anyhow::Result;
use std::path::Path;
use xmtp_db::{
    EncryptedMessageStore, NativeDb, StorageOption, diesel::connection::SimpleConnection,
};

pub fn db_vacuum(source: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<()> {
    let snapshot = std::fs::read(source)?;

    let eph_db = NativeDb::new_unencrypted(&StorageOption::Ephemeral)?;
    let eph_store = EncryptedMessageStore::new(eph_db)?;
    eph_store.db().raw_query_write(|conn| {
        conn.deserialize_readonly_database_from_buffer(&snapshot)?;
        conn.batch_execute(&format!(
            "VACUUM INTO '{}'",
            dest.as_ref().to_string_lossy()
        ))?;
        Ok(())
    })?;

    Ok(())
}
