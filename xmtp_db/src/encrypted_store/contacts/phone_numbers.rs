use super::StoredContact;
use crate::encrypted_store::schema::contact_phone_numbers;
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
#[diesel(table_name = contact_phone_numbers)]
#[diesel(primary_key(id))]
#[diesel(belongs_to(StoredContact, foreign_key = contact_id))]
pub struct StoredContactPhoneNumber {
    pub id: i32,
    pub contact_id: i32,
    pub phone_number: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = contact_phone_numbers)]
pub struct NewContactPhoneNumber {
    pub contact_id: i32,
    pub phone_number: String,
    pub label: Option<String>,
}

impl_store!(NewContactPhoneNumber, contact_phone_numbers);
