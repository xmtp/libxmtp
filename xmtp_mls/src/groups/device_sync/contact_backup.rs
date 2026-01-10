//! Contact backup extension trait for device sync.
//! Provides additional methods for ContactSave proto type.

use xmtp_db::encrypted_store::contacts::ContactData;
use xmtp_proto::xmtp::device_sync::contact_backup::ContactSave;

/// Extension trait for ContactSave to add conversion methods
pub trait ContactSaveExt {
    /// Convert to ContactData for creating/updating a contact
    fn to_contact_data(&self) -> ContactData;
}

impl ContactSaveExt for ContactSave {
    fn to_contact_data(&self) -> ContactData {
        ContactData {
            display_name: self.display_name.clone(),
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            prefix: self.prefix.clone(),
            suffix: self.suffix.clone(),
            company: self.company.clone(),
            job_title: self.job_title.clone(),
            birthday: self.birthday.clone(),
            note: self.note.clone(),
            image_url: self.image_url.clone(),
            is_favorite: Some(self.is_favorite),
        }
    }
}
