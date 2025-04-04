#![allow(clippy::unwrap_used)]

use crate::{DbConnection, EncryptedMessageStore, StorageOption};
use xmtp_common::tmp_path;

impl EncryptedMessageStore {
    pub fn generate_enc_key() -> [u8; 32] {
        xmtp_common::rand_array::<32>()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn remove_db_files<P: AsRef<str>>(path: P) {
        use crate::EncryptedConnection;

        let path = path.as_ref();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(EncryptedConnection::salt_file(path).unwrap()).unwrap();
    }

    /// just a no-op on wasm32
    #[cfg(target_arch = "wasm32")]
    pub fn remove_db_files<P: AsRef<str>>(_path: P) {}
}

/// Test harness that loads an Ephemeral store.
pub async fn with_connection<F, R>(fun: F) -> R
where
    F: FnOnce(&DbConnection) -> R,
{
    let store = EncryptedMessageStore::new(
        StorageOption::Ephemeral,
        EncryptedMessageStore::generate_enc_key(),
    )
    .await
    .unwrap();
    let conn = &store.conn().expect("acquiring a Connection failed");
    fun(conn)
}

/// Test harness that loads an Ephemeral store.
pub async fn with_connection_async<F, T, R>(fun: F) -> R
where
    F: FnOnce(DbConnection) -> T,
    T: Future<Output = R>,
{
    let store = EncryptedMessageStore::new(
        StorageOption::Ephemeral,
        EncryptedMessageStore::generate_enc_key(),
    )
    .await
    .unwrap();
    let conn = store.conn().expect("acquiring a Connection failed");
    fun(conn).await
}

impl EncryptedMessageStore {
    pub async fn new_test() -> Self {
        let tmp_path = tmp_path();
        EncryptedMessageStore::new(
            StorageOption::Persistent(tmp_path),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .expect("constructing message store failed.")
    }

    pub async fn new_test_with_path(path: &str) -> Self {
        EncryptedMessageStore::new(StorageOption::Persistent(path.to_string()), [0u8; 32])
            .await
            .expect("constructing message store failed.")
    }
}
