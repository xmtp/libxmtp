use xmtp_db::{
    StorageError,
    encrypted_store::contacts::{Email, QueryContacts},
};

use crate::context::XmtpSharedContext;

pub struct ContactEmail<Context> {
    context: Context,
    id: i32,
    pub email: String,
    pub label: Option<String>,
}

impl<Context: XmtpSharedContext> ContactEmail<Context> {
    pub(crate) fn new(context: Context, data: Email) -> Self {
        Self {
            context,
            id: data.id,
            email: data.email,
            label: data.label,
        }
    }

    pub fn update(&self, email: String, label: Option<String>) -> Result<(), StorageError> {
        self.context.db().update_email(self.id, email, label)
    }

    pub fn delete(&self) -> Result<(), StorageError> {
        self.context.db().delete_email(self.id)
    }
}
