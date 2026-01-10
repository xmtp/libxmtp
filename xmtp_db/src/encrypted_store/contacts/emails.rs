use super::StoredContact;
use crate::encrypted_store::schema::contact_emails;
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
#[diesel(table_name = contact_emails)]
#[diesel(primary_key(id))]
#[diesel(belongs_to(StoredContact, foreign_key = contact_id))]
pub struct StoredContactEmail {
    pub id: i32,
    pub contact_id: i32,
    pub email: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = contact_emails)]
pub struct NewContactEmail {
    pub contact_id: i32,
    pub email: String,
    pub label: Option<String>,
}

impl_store!(NewContactEmail, contact_emails);
