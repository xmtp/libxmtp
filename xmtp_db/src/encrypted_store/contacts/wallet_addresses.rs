use super::StoredContact;
use crate::encrypted_store::schema::contact_wallet_addresses;
use crate::impl_store;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Selectable,
    Associations,
    PartialEq,
    Eq,
)]
#[diesel(table_name = contact_wallet_addresses)]
#[diesel(primary_key(id))]
#[diesel(belongs_to(StoredContact, foreign_key = contact_id))]
pub struct StoredContactWalletAddress {
    pub id: i32,
    pub contact_id: i32,
    pub wallet_address: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = contact_wallet_addresses)]
pub struct NewContactWalletAddress {
    pub contact_id: i32,
    pub wallet_address: String,
    pub label: Option<String>,
}

impl_store!(NewContactWalletAddress, contact_wallet_addresses);
