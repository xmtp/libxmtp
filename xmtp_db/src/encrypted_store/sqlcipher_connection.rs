//! SQLCipher-specific Connection
use diesel::{
    connection::{LoadConnection, SimpleConnection},
    deserialize::FromSqlRow,
    prelude::*,
    sql_query,
};
use std::{
    fmt::Display,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::{NotFound, native::NativeStorageError};

use super::{EncryptionKey, StorageOption};

pub type Salt = [u8; 16];
const PLAINTEXT_HEADER_SIZE: usize = 32;
const SALT_FILE_NAME: &str = "sqlcipher_salt";

// For PRAGMA query log statements
#[derive(QueryableByName, Debug)]
struct CipherVersion {
    #[diesel(sql_type = diesel::sql_types::Text)]
    cipher_version: String,
}

// For PRAGMA query log statements
#[derive(QueryableByName, Debug)]
struct CipherProviderVersion {
    #[diesel(sql_type = diesel::sql_types::Text)]
    cipher_provider_version: String,
}

/// Specialized Connection for r2d2 connection pool.
#[derive(Clone, Debug, zeroize::ZeroizeOnDrop)]
pub struct EncryptedConnection {
    key: EncryptionKey,
    /// We don't store the salt for Ephemeral Dbs
    salt: Option<Salt>,
}

impl EncryptedConnection {
    /// Creates a file for the salt and stores it
    pub fn new(key: EncryptionKey, opts: &StorageOption) -> Result<Self, NativeStorageError> {
        use super::StorageOption::*;

        let salt = match opts {
            Ephemeral => None,
            Persistent(db_path) => {
                Self::check_for_sqlcipher(opts, None)?;
                let mut salt = [0u8; 16];
                let db_pathbuf = PathBuf::from(db_path);
                let salt_path = Self::salt_file(db_path)?;

                match (salt_path.try_exists()?, db_pathbuf.try_exists()?) {
                    // db and salt exist
                    (true, true) => {
                        tracing::debug!(
                            salt = %salt_path.display(),
                            db = %db_pathbuf.display(),
                            "salt and database exist, db=[{}], salt=[{}]",
                            db_pathbuf.display(),
                            salt_path.display(),
                        );
                        let file = File::open(salt_path)?;
                        salt = <Salt as hex::FromHex>::from_hex(
                            file.bytes().take(32).collect::<Result<Vec<u8>, _>>()?,
                        )?;
                    }
                    // the db exists and needs to be migrated
                    (false, true) => {
                        tracing::debug!(
                            "migrating sqlcipher db=[{}] to plaintext header with salt=[{}]",
                            db_pathbuf.display(),
                            salt_path.display()
                        );
                        Self::migrate(db_path, key, &mut salt)?;
                    }
                    // the db doesn't exist yet and needs to be created
                    (false, false) => {
                        tracing::debug!(
                            "creating new sqlcipher db=[{}] with salt=[{}]",
                            db_pathbuf.display(),
                            salt_path.display()
                        );
                        Self::create(db_path, key, &mut salt)?;
                    }
                    // the db doesn't exist but the salt does
                    // This generally doesn't make sense & shouldn't happen.
                    // Create a new database and delete the salt file.
                    (true, false) => {
                        tracing::debug!(
                            "database [{}] does not exist, but the salt [{}] does, re-creating",
                            db_pathbuf.display(),
                            salt_path.display(),
                        );
                        std::fs::remove_file(salt_path)?;
                        Self::create(db_path, key, &mut salt)?;
                    }
                }
                Some(salt)
            }
        };

        Ok(Self { key, salt })
    }

    /// create a new database + salt file.
    /// writes the 16-bytes hex-encoded salt to `salt`
    fn create(
        path: &String,
        key: EncryptionKey,
        salt: &mut [u8],
    ) -> Result<(), NativeStorageError> {
        let conn = &mut SqliteConnection::establish(path)?;
        conn.batch_execute(&format!(
            r#"
            {}
            {}
            PRAGMA journal_mode = WAL;
        "#,
            pragma_key(hex::encode(key)),
            pragma_plaintext_header()
        ))?;

        Self::write_salt(path, conn, salt)?;
        Ok(())
    }

    /// Executes the steps outlined in the [SQLCipher Docs](https://www.zetetic.net/sqlcipher/sqlcipher-api/#cipher_plaintext_header_size)
    /// Migrates the database to `cipher_plaintext_header_size` and returns the salt after
    /// persisting it to SALT_FILE_NAME.
    ///
    /// if the salt file already exists, deletes it.
    fn migrate(
        path: &String,
        key: EncryptionKey,
        salt: &mut [u8],
    ) -> Result<(), NativeStorageError> {
        let conn = &mut SqliteConnection::establish(path)?;

        conn.batch_execute(&format!(
            r#"
            {}
            select count(*) from sqlite_master; -- trigger header read, currently it is encrypted
        "#,
            pragma_key(hex::encode(key))
        ))?;

        // get the salt and save it for later use
        Self::write_salt(path, conn, salt)?;

        conn.batch_execute(&format!(
            r#"
            {}
            PRAGMA user_version = 1; -- force header write
        "#,
            pragma_plaintext_header()
        ))?;

        Ok(())
    }

    /// Get the salt from the opened database, write it to `Self::salt_file(db_path)` as hex-encoded
    /// bytes, and then copy it to `buf` after decoding hex bytes.
    fn write_salt(
        path: &String,
        conn: &mut SqliteConnection,
        buf: &mut [u8],
    ) -> Result<(), NativeStorageError> {
        let mut row_iter = conn.load(sql_query("PRAGMA cipher_salt"))?;
        // cipher salt should always exist. if it doesn't SQLCipher is misconfigured.
        let row = row_iter
            .next()
            .ok_or(NotFound::CipherSalt(path.to_string()))??;
        let salt = <String as FromSqlRow<diesel::sql_types::Text, _>>::build_from_row(&row)?;
        tracing::debug!(
            salt,
            file = %Self::salt_file(PathBuf::from(path))?.display(),
            "writing salt to file"
        );
        let mut f = File::create(Self::salt_file(PathBuf::from(path))?)?;

        f.write_all(salt.as_bytes())?;
        let mut perms = f.metadata()?.permissions();
        perms.set_readonly(true);
        f.set_permissions(perms)?;

        let salt = hex::decode(salt)?;
        buf.copy_from_slice(&salt);
        Ok(())
    }

    /// Salt file is stored next to the sqlite3 db3 file as `{db_file_name}.SALT_FILE_NAME`.
    /// If the db file is named `sqlite3_xmtp_db.db3`, the salt file would
    /// be stored next to this file as `sqlite3_xmtp_db.db3.sqlcipher_salt`
    pub(crate) fn salt_file<P: AsRef<Path>>(db_path: P) -> std::io::Result<PathBuf> {
        let db_path: &Path = db_path.as_ref();
        let name = db_path.file_name().ok_or(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "database file has no name",
        ))?;
        let db_path = db_path.parent().ok_or(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Parent directory could not be found",
        ))?;
        Ok(db_path.join(format!("{}.{}", name.to_string_lossy(), SALT_FILE_NAME)))
    }

    /// Output the corect order of PRAGMAS to instantiate a connection
    fn pragmas(&self) -> impl Display {
        let Self { key, salt } = self;

        if let Some(s) = salt {
            format!(
                "{}\n{}\n{}",
                pragma_key(hex::encode(key)),
                pragma_plaintext_header(),
                pragma_salt(hex::encode(s))
            )
        } else {
            format!(
                "{}\n{}",
                pragma_key(hex::encode(key)),
                pragma_plaintext_header()
            )
        }
    }

    fn check_for_sqlcipher(
        opts: &StorageOption,
        conn: Option<&mut SqliteConnection>,
    ) -> Result<CipherVersion, NativeStorageError> {
        if let Some(path) = opts.path() {
            let exists = std::path::Path::new(path).exists();
            tracing::debug!("db @ [{}] exists? [{}]", path, exists);
        }
        let conn = if let Some(c) = conn {
            c
        } else {
            &mut opts.conn()?
        };
        let mut cipher_version = sql_query("PRAGMA cipher_version").load::<CipherVersion>(conn)?;
        if cipher_version.is_empty() {
            return Err(NativeStorageError::SqlCipherNotLoaded);
        }
        Ok(cipher_version.pop().expect("checked for empty"))
    }
}

impl super::native::ValidatedConnection for EncryptedConnection {
    fn validate(&self, opts: &StorageOption) -> Result<(), NativeStorageError> {
        let conn = &mut opts.conn()?;
        let sqlcipher_version = EncryptedConnection::check_for_sqlcipher(opts, Some(conn))?;

        // test the key according to
        // https://www.zetetic.net/sqlcipher/sqlcipher-api/#testing-the-key
        conn.batch_execute(&format!(
            "{}
            SELECT count(*) FROM sqlite_master;",
            self.pragmas()
        ))
        .map_err(|_| NativeStorageError::SqlCipherKeyIncorrect)?;

        let CipherProviderVersion {
            cipher_provider_version,
        } = sql_query("PRAGMA cipher_provider_version")
            .get_result::<CipherProviderVersion>(conn)?;
        tracing::info!(
            "Sqlite cipher_version={:?}, cipher_provider_version={:?}",
            sqlcipher_version.cipher_version,
            cipher_provider_version
        );
        let log = std::env::var("SQLCIPHER_LOG");
        let is_sqlcipher_log_enabled = matches!(log, Ok(s) if s == "true" || s == "1");
        if is_sqlcipher_log_enabled {
            conn.batch_execute("PRAGMA cipher_log = stderr; PRAGMA cipher_log_level = INFO;")
                .ok();
        }
        tracing::debug!("SQLCipher Database validated.");
        Ok(())
    }
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for EncryptedConnection
{
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        conn.batch_execute(&format!(
            "{}
            PRAGMA query_only = ON;
            PRAGMA busy_timeout = 5000;",
            self.pragmas()
        ))
        .map_err(diesel::r2d2::Error::QueryError)?;

        Ok(())
    }
}

fn pragma_key(key: impl Display) -> impl Display {
    format!(r#"PRAGMA key = "x'{key}'";"#)
}

fn pragma_salt(salt: impl Display) -> impl Display {
    format!(r#"PRAGMA cipher_salt="x'{salt}'";"#)
}

fn pragma_plaintext_header() -> impl Display {
    format!(r#"PRAGMA cipher_plaintext_header_size={PLAINTEXT_HEADER_SIZE};"#)
}

#[cfg(test)]
mod tests {
    use crate::EncryptedMessageStore;
    use diesel_migrations::MigrationHarness;
    use std::fs::File;
    use xmtp_common::tmp_path;

    use super::*;
    const SQLITE3_PLAINTEXT_HEADER: &str = "SQLite format 3\0";
    use StorageOption::*;

    #[tokio::test]
    async fn test_sqlcipher_version() {
        let db_path = tmp_path();
        {
            let opts = Persistent(db_path.clone());
            let v = EncryptedConnection::check_for_sqlcipher(&opts, None).unwrap();
            println!("SQLCipher Version {}", v.cipher_version);
        }
    }

    #[tokio::test]
    async fn test_db_creates_with_plaintext_header() {
        let db_path = tmp_path();
        {
            let _ = EncryptedMessageStore::new(
                Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .await
            .unwrap();

            assert!(EncryptedConnection::salt_file(&db_path).unwrap().exists());
            let bytes = std::fs::read(EncryptedConnection::salt_file(&db_path).unwrap()).unwrap();
            let salt = hex::decode(bytes).unwrap();
            assert_eq!(salt.len(), 16);

            let mut plaintext_header = [0; 16];
            let mut file = File::open(&db_path).unwrap();
            file.read_exact(&mut plaintext_header).unwrap();

            assert_eq!(
                SQLITE3_PLAINTEXT_HEADER,
                String::from_utf8(plaintext_header.into()).unwrap()
            );
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn test_db_migrates() {
        let db_path = tmp_path();
        {
            let key = EncryptedMessageStore::generate_enc_key();
            {
                let conn = &mut SqliteConnection::establish(&db_path).unwrap();
                conn.batch_execute(&format!(
                    r#"
            {}
            PRAGMA busy_timeout = 5000;
            PRAGMA journal_mode = WAL;
            "#,
                    pragma_key(hex::encode(key))
                ))
                .unwrap();
                conn.run_pending_migrations(crate::MIGRATIONS).unwrap();
            }

            // no plaintext header before migration
            let mut plaintext_header = [0; 16];
            let mut file = File::open(&db_path).unwrap();
            file.read_exact(&mut plaintext_header).unwrap();
            assert!(String::from_utf8_lossy(&plaintext_header) != SQLITE3_PLAINTEXT_HEADER);

            let _ = EncryptedMessageStore::new(Persistent(db_path.clone()), key)
                .await
                .unwrap();

            assert!(EncryptedConnection::salt_file(&db_path).unwrap().exists());
            let bytes = std::fs::read(EncryptedConnection::salt_file(&db_path).unwrap()).unwrap();
            let salt = hex::decode(bytes).unwrap();
            assert_eq!(salt.len(), 16);

            let mut plaintext_header = [0; 16];
            let mut file = File::open(&db_path).unwrap();
            file.read_exact(&mut plaintext_header).unwrap();

            assert_eq!(
                SQLITE3_PLAINTEXT_HEADER,
                String::from_utf8(plaintext_header.into()).unwrap()
            );
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }
}
