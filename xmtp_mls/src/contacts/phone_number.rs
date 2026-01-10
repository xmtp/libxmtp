use xmtp_db::{
    StorageError,
    encrypted_store::contacts::{PhoneNumber, QueryContacts},
};

use crate::context::XmtpSharedContext;

pub struct ContactPhoneNumber<Context> {
    context: Context,
    id: i32,
    pub phone_number: String,
    pub label: Option<String>,
}

impl<Context: XmtpSharedContext> ContactPhoneNumber<Context> {
    pub(crate) fn new(context: Context, data: PhoneNumber) -> Self {
        Self {
            context,
            id: data.id,
            phone_number: data.phone_number,
            label: data.label,
        }
    }

    pub fn update(&self, phone_number: String, label: Option<String>) -> Result<(), StorageError> {
        self.context
            .db()
            .update_phone_number(self.id, phone_number, label)
    }

    pub fn delete(&self) -> Result<(), StorageError> {
        self.context.db().delete_phone_number(self.id)
    }
}
