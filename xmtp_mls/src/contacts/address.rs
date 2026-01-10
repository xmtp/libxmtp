use xmtp_db::{
    StorageError,
    encrypted_store::contacts::{AddressData, QueryContacts},
};

use crate::context::XmtpSharedContext;

pub struct ContactAddress<Context> {
    context: Context,
    id: i32,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub address3: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub label: Option<String>,
}

impl<Context: XmtpSharedContext> ContactAddress<Context> {
    pub(crate) fn new(context: Context, data: AddressData) -> Option<Self> {
        Some(Self {
            context,
            id: data.id?,
            address1: data.address1,
            address2: data.address2,
            address3: data.address3,
            city: data.city,
            region: data.region,
            postal_code: data.postal_code,
            country: data.country,
            label: data.label,
        })
    }

    pub fn update(&self, data: AddressData) -> Result<(), StorageError> {
        self.context.db().update_address(self.id, data)
    }

    pub fn delete(&self) -> Result<(), StorageError> {
        self.context.db().delete_address(self.id)
    }
}
