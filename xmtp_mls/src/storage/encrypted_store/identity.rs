use super::{schema::identity, DbConnection, StorageError};
use crate::{
    identity::Identity,
    storage::serialization::{db_deserialize, db_serialize},
    Fetch, Store,
};
use diesel::prelude::*;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = identity)]
pub struct StoredIdentity {
    pub account_address: String,
    pub installation_keys: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    rowid: Option<i32>,
}

impl StoredIdentity {
    pub fn new(
        account_address: String,
        installation_keys: Vec<u8>,
        credential_bytes: Vec<u8>,
    ) -> Self {
        Self {
            account_address,
            installation_keys,
            credential_bytes,
            rowid: None,
        }
    }
}

impl From<&Identity> for StoredIdentity {
    fn from(identity: &Identity) -> Self {
        StoredIdentity {
            account_address: identity.account_address.clone(),
            installation_keys: db_serialize(&identity.installation_keys).unwrap(),
            credential_bytes: db_serialize(&identity.credential).unwrap(),
            rowid: None,
        }
    }
}

impl From<StoredIdentity> for Identity {
    fn from(identity: StoredIdentity) -> Self {
        Identity {
            account_address: identity.account_address,
            installation_keys: db_deserialize(&identity.installation_keys).unwrap(),
            credential: db_deserialize(&identity.credential_bytes).unwrap(),
        }
    }
}

impl Store<DbConnection> for StoredIdentity {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(identity::table)
            .values(self)
            .execute(into)?;
        Ok(())
    }
}

impl Fetch<StoredIdentity> for DbConnection {
    type Key = ();
    fn fetch(&mut self, _key: ()) -> Result<Option<StoredIdentity>, StorageError> where {
        use super::schema::identity::dsl::*;
        Ok(identity.first(self).optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{tests::rand_vec, EncryptedMessageStore, StorageOption},
        StoredIdentity,
    };
    use crate::Store;

    #[test]
    fn can_only_store_one_identity() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = &mut store.conn().unwrap();

        StoredIdentity::new("".to_string(), rand_vec(), rand_vec())
            .store(conn)
            .unwrap();

        let duplicate_insertion =
            StoredIdentity::new("".to_string(), rand_vec(), rand_vec()).store(conn);
        assert!(duplicate_insertion.is_err());
    }
}
