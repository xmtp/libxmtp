use xmtp_db::{
    StorageError,
    encrypted_store::contacts::{QueryContacts, Url},
};

use crate::context::XmtpSharedContext;

pub struct ContactUrl<Context> {
    context: Context,
    id: i32,
    pub url: String,
    pub label: Option<String>,
}

impl<Context: XmtpSharedContext> ContactUrl<Context> {
    pub(crate) fn new(context: Context, data: Url) -> Self {
        Self {
            context,
            id: data.id,
            url: data.url,
            label: data.label,
        }
    }

    pub fn update(&self, url: String, label: Option<String>) -> Result<(), StorageError> {
        self.context.db().update_url(self.id, url, label)
    }

    pub fn delete(&self) -> Result<(), StorageError> {
        self.context.db().delete_url(self.id)
    }
}
