use super::StoredContact;
use crate::encrypted_store::schema::contact_urls;
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
#[diesel(table_name = contact_urls)]
#[diesel(primary_key(id))]
#[diesel(belongs_to(StoredContact, foreign_key = contact_id))]
pub struct StoredContactUrl {
    pub id: i32,
    pub contact_id: i32,
    pub url: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = contact_urls)]
pub struct NewContactUrl {
    pub contact_id: i32,
    pub url: String,
    pub label: Option<String>,
}

impl_store!(NewContactUrl, contact_urls);
