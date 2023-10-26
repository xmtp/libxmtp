use diesel::prelude::*;

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
