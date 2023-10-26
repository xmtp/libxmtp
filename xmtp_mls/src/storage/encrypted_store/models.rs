use diesel::prelude::*;

use crate::{
    identity::Identity,
    storage::serialization::{db_deserialize, db_serialize},
};

use super::schema::*;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = openmls_key_store)]
#[diesel(primary_key(key_bytes))]
pub struct StoredKeyStoreEntry {
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
}

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = identity)]
pub struct StoredIdentity {
    pub account_address: String,
    pub signature_keypair: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    rowid: Option<i32>,
}

impl StoredIdentity {
    pub fn new(
        account_address: String,
        signature_keypair: Vec<u8>,
        credential_bytes: Vec<u8>,
    ) -> Self {
        Self {
            account_address,
            signature_keypair,
            credential_bytes,
            rowid: None,
        }
    }
}

impl From<Identity> for StoredIdentity {
    fn from(identity: Identity) -> Self {
        StoredIdentity {
            account_address: identity.account_address,
            signature_keypair: db_serialize(&identity.signer).unwrap(),
            credential_bytes: db_serialize(&identity.credential).unwrap(),
            rowid: None,
        }
    }
}

impl Into<Identity> for StoredIdentity {
    fn into(self) -> Identity {
        Identity {
            account_address: self.account_address,
            signer: db_deserialize(&self.signature_keypair).unwrap(),
            credential: db_deserialize(&self.credential_bytes).unwrap(),
        }
    }
}
