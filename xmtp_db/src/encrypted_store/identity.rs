use crate::encrypted_store::schema::identity;
use derive_builder::Builder;
use diesel::prelude::*;

use crate::{impl_fetch, impl_store};

/// Identity of this installation
/// There can only be one.
#[derive(Insertable, Queryable, Debug, Clone, Builder)]
#[diesel(table_name = identity)]
#[builder(setter(into), build_fn(error = "crate::StorageError"))]
pub struct StoredIdentity {
    pub inbox_id: String,
    pub installation_keys: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    #[builder(setter(skip))]
    rowid: Option<i32>,
}

impl_fetch!(StoredIdentity, identity);
impl_store!(StoredIdentity, identity);

impl StoredIdentity {
    pub fn builder() -> StoredIdentityBuilder {
        StoredIdentityBuilder::default()
    }

    pub fn new(inbox_id: String, installation_keys: Vec<u8>, credential_bytes: Vec<u8>) -> Self {
        Self {
            inbox_id,
            installation_keys,
            credential_bytes,
            rowid: None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::{
        super::{EncryptedMessageStore, StorageOption},
        StoredIdentity,
    };
    use crate::Store;
    use xmtp_common::rand_vec;

    #[xmtp_common::test]
    async fn can_only_store_one_identity() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
        let conn = &store.conn().unwrap();

        StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>())
            .store(conn)
            .unwrap();

        let duplicate_insertion =
            StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>()).store(conn);
        assert!(duplicate_insertion.is_err());
    }
}
