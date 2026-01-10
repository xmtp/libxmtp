use super::StoredContact;
use crate::encrypted_store::schema::contact_addresses;
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
#[diesel(table_name = contact_addresses)]
#[diesel(primary_key(id))]
#[diesel(belongs_to(StoredContact, foreign_key = contact_id))]
pub struct StoredContactAddress {
    pub id: i32,
    pub contact_id: i32,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub address3: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = contact_addresses)]
pub struct NewContactAddress {
    pub contact_id: i32,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub address3: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub label: Option<String>,
}

impl_store!(NewContactAddress, contact_addresses);

/// Address data used for add/update operations and in FullContact responses
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddressData {
    /// The id of the address (None when creating, Some when retrieved from DB)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub address3: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub label: Option<String>,
}

impl From<AddressData> for NewContactAddress {
    fn from(data: AddressData) -> Self {
        Self {
            contact_id: 0, // Will be set by the add function
            address1: data.address1,
            address2: data.address2,
            address3: data.address3,
            city: data.city,
            region: data.region,
            postal_code: data.postal_code,
            country: data.country,
            label: data.label,
        }
    }
}
