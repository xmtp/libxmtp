use crate::storage::encrypted_store::schema::identity_inbox;
use diesel::prelude::*;
use xmtp_id::InboxId;

use crate::{
    identity::Identity,
    impl_fetch, impl_store,
    storage::serialization::{db_deserialize, db_serialize},
};

/// Identity of this installation
/// There can only be one.
#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = identity_inbox)]
pub struct StoredIdentity {
    pub inbox_id: InboxId,
    pub installation_keys: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    rowid: Option<i32>,
}

impl_fetch!(StoredIdentity, identity_inbox);
impl_store!(StoredIdentity, identity_inbox);

impl StoredIdentity {
    pub fn new(inbox_id: InboxId, installation_keys: Vec<u8>, credential_bytes: Vec<u8>) -> Self {
        Self {
            inbox_id,
            installation_keys,
            credential_bytes,
            rowid: None,
        }
    }
}

impl From<&Identity> for StoredIdentity {
    fn from(identity: &Identity) -> Self {
        StoredIdentity {
            inbox_id: identity.inbox_id.clone(),
            installation_keys: db_serialize(&identity.installation_keys).unwrap(),
            credential_bytes: db_serialize(&identity.credential()).unwrap(),
            rowid: None,
        }
    }
}

impl From<StoredIdentity> for Identity {
    fn from(identity: StoredIdentity) -> Self {
        Identity {
            inbox_id: identity.inbox_id.clone(),
            installation_keys: db_deserialize(&identity.installation_keys).unwrap(),
            credential: db_deserialize(&identity.credential_bytes).unwrap(),
            signature_request: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{EncryptedMessageStore, StorageOption},
        StoredIdentity,
    };
    use crate::{utils::test::rand_vec, Store};

    #[test]
    fn can_only_store_one_identity() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = &store.conn().unwrap();

        StoredIdentity::new("".to_string(), rand_vec(), rand_vec())
            .store(conn)
            .unwrap();

        let duplicate_insertion =
            StoredIdentity::new("".to_string(), rand_vec(), rand_vec()).store(conn);
        assert!(duplicate_insertion.is_err());
    }
}
