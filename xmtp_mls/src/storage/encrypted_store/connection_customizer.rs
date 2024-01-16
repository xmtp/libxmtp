use super::EncryptionKey;
use diesel::connection::SimpleConnection;
use diesel::{r2d2::CustomizeConnection, SqliteConnection};

#[derive(Debug, Clone)]
pub struct ConnectionCustomizer {
    encryption_key: Option<EncryptionKey>,
}

impl ConnectionCustomizer {
    pub fn new(encryption_key: Option<EncryptionKey>) -> Self {
        Self { encryption_key }
    }
}

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionCustomizer {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        conn.batch_execute("PRAGMA journal_mode = WAL;")
            .map_err(diesel::r2d2::Error::QueryError)?;

        if let Some(encryption_key) = self.encryption_key {
            conn.batch_execute(&format!(
                "PRAGMA key = \"x'{}'\";",
                hex::encode(encryption_key)
            ))
            .map_err(diesel::r2d2::Error::QueryError)?;
        }

        Ok(())
    }
}
