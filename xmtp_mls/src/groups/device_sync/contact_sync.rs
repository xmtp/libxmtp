//! Contact sync module for device sync.
//! Handles exporting and importing contacts during device sync.

use super::DeviceSyncError;
use prost::Message;
use serde::{Deserialize, Serialize};
use xmtp_db::encrypted_store::contacts::{AddressData, QueryContacts};
use xmtp_proto::xmtp::device_sync::contact_backup::ContactSave;

const CONTACTS_BATCH_SIZE: i64 = 100;

/// Wrapper for a list of contacts to be serialized
#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct ContactsSave {
    #[prost(message, repeated, tag = "1")]
    pub contacts: Vec<ContactSave>,
}

/// Export all contacts to bytes for device sync.
pub fn export_contacts<D: QueryContacts>(db: &D) -> Result<Vec<u8>, DeviceSyncError> {
    let mut all_contacts = Vec::new();
    let mut offset = 0;

    loop {
        let batch = db.contacts_paged(CONTACTS_BATCH_SIZE, offset)?;
        if batch.is_empty() {
            break;
        }
        all_contacts.extend(batch);
        offset += CONTACTS_BATCH_SIZE;
    }

    let saves: Vec<ContactSave> = all_contacts.into_iter().map(Into::into).collect();
    let contacts_save = ContactsSave { contacts: saves };

    let mut bytes = Vec::new();
    contacts_save.encode(&mut bytes)?;
    Ok(bytes)
}

/// Import contacts from bytes during device sync.
pub fn import_contacts<D: QueryContacts>(db: &D, data: &[u8]) -> Result<usize, DeviceSyncError> {
    if data.is_empty() {
        return Ok(0);
    }

    let contacts_save = ContactsSave::decode(data)?;
    let mut imported_count = 0;

    for contact_save in contacts_save.contacts {
        if let Err(err) = import_single_contact(db, contact_save) {
            tracing::warn!("Failed to import contact: {err:?}");
        } else {
            imported_count += 1;
        }
    }

    Ok(imported_count)
}

/// Import a single contact from a ContactSave proto.
/// This is used by both the byte-based import and the archive import.
pub fn import_single_contact<D: QueryContacts>(
    db: &D,
    contact_save: ContactSave,
) -> Result<(), DeviceSyncError> {
    use super::contact_backup::ContactSaveExt;

    let inbox_id = &contact_save.inbox_id;
    let contact_data = contact_save.to_contact_data();

    // Check if contact already exists
    let existing = db.get_contact(inbox_id)?;

    if let Some(existing_contact) = existing {
        // Update existing contact if the incoming one is newer
        if contact_save.updated_at_ns > existing_contact.updated_at_ns {
            db.update_contact(inbox_id, contact_data)?;

            // Delete existing child data and re-add from sync
            // This ensures we get the exact same data as the source
            delete_all_child_data(db, inbox_id)?;
            add_child_data(db, inbox_id, &contact_save)?;
        }
    } else {
        // Create new contact
        db.add_contact(inbox_id, contact_data)?;
        add_child_data(db, inbox_id, &contact_save)?;
    }

    Ok(())
}

fn delete_all_child_data<D: QueryContacts>(db: &D, inbox_id: &str) -> Result<(), DeviceSyncError> {
    // Delete phone numbers
    for phone in db.get_phone_numbers(inbox_id)? {
        db.delete_phone_number(phone.id)?;
    }

    // Delete emails
    for email in db.get_emails(inbox_id)? {
        db.delete_email(email.id)?;
    }

    // Delete URLs
    for url in db.get_urls(inbox_id)? {
        db.delete_url(url.id)?;
    }

    // Delete wallet addresses
    for wallet in db.get_wallet_addresses(inbox_id)? {
        db.delete_wallet_address(wallet.id)?;
    }

    // Delete addresses
    for addr in db.get_addresses(inbox_id)? {
        if let Some(id) = addr.id {
            db.delete_address(id)?;
        }
    }

    Ok(())
}

fn add_child_data<D: QueryContacts>(
    db: &D,
    inbox_id: &str,
    contact_save: &ContactSave,
) -> Result<(), DeviceSyncError> {
    // Add phone numbers
    for phone in &contact_save.phone_numbers {
        db.add_phone_number(inbox_id, phone.phone_number.clone(), phone.label.clone())?;
    }

    // Add emails
    for email in &contact_save.emails {
        db.add_email(inbox_id, email.email.clone(), email.label.clone())?;
    }

    // Add URLs
    for url in &contact_save.urls {
        db.add_url(inbox_id, url.url.clone(), url.label.clone())?;
    }

    // Add wallet addresses
    for wallet in &contact_save.wallet_addresses {
        db.add_wallet_address(
            inbox_id,
            wallet.wallet_address.clone(),
            wallet.label.clone(),
        )?;
    }

    // Add addresses
    for addr in &contact_save.addresses {
        let addr_data: AddressData = addr.clone().into();
        db.add_address(inbox_id, addr_data)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use xmtp_db::encrypted_store::contacts::ContactData;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_contact_export_import() {
        tester!(alix);

        // Create a contact with child data
        let contact_data = ContactData {
            display_name: Some("Test User".to_string()),
            first_name: Some("Test".to_string()),
            last_name: Some("User".to_string()),
            is_favorite: Some(true),
            ..Default::default()
        };

        alix.db().add_contact("inbox_test_123", contact_data)?;
        alix.db().add_phone_number(
            "inbox_test_123",
            "555-1234".to_string(),
            Some("Mobile".to_string()),
        )?;
        alix.db().add_email(
            "inbox_test_123",
            "test@example.com".to_string(),
            Some("Work".to_string()),
        )?;
        alix.db()
            .add_url("inbox_test_123", "https://example.com".to_string(), None)?;
        alix.db().add_wallet_address(
            "inbox_test_123",
            "0x1234567890abcdef".to_string(),
            Some("Main".to_string()),
        )?;
        alix.db().add_address(
            "inbox_test_123",
            AddressData {
                address1: Some("123 Main St".to_string()),
                city: Some("Springfield".to_string()),
                ..Default::default()
            },
        )?;

        // Export contacts
        let exported = export_contacts(&alix.db())?;
        assert!(!exported.is_empty());

        // Create a second installation
        tester!(alix2, from: alix);

        // Verify no contacts exist on alix2
        let contacts_before = alix2.db().get_contacts(None)?;
        assert!(contacts_before.is_empty());

        // Import contacts
        let imported_count = import_contacts(&alix2.db(), &exported)?;
        assert_eq!(imported_count, 1);

        // Verify contact was imported
        let contact = alix2.db().get_contact("inbox_test_123")?;
        assert!(contact.is_some());
        let contact = contact.unwrap();
        assert_eq!(contact.display_name, Some("Test User".to_string()));
        assert_eq!(contact.first_name, Some("Test".to_string()));
        assert!(contact.is_favorite);

        // Verify child data
        assert_eq!(contact.phone_numbers.len(), 1);
        assert_eq!(contact.phone_numbers[0].phone_number, "555-1234");

        assert_eq!(contact.emails.len(), 1);
        assert_eq!(contact.emails[0].email, "test@example.com");

        assert_eq!(contact.urls.len(), 1);
        assert_eq!(contact.urls[0].url, "https://example.com");

        assert_eq!(contact.wallet_addresses.len(), 1);
        assert_eq!(
            contact.wallet_addresses[0].wallet_address,
            "0x1234567890abcdef"
        );

        assert_eq!(contact.addresses.len(), 1);
        assert_eq!(
            contact.addresses[0].address1,
            Some("123 Main St".to_string())
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_contact_import_update() {
        tester!(alix);
        tester!(alix2, from: alix);

        // Create a contact on alix2 first (older)
        let contact_data_old = ContactData {
            display_name: Some("Old Name".to_string()),
            ..Default::default()
        };
        alix2
            .db()
            .add_contact("inbox_update_test", contact_data_old)?;

        // Wait a bit to ensure timestamp difference
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Create a contact on alix (newer)
        let contact_data = ContactData {
            display_name: Some("Updated Name".to_string()),
            ..Default::default()
        };
        alix.db().add_contact("inbox_update_test", contact_data)?;

        // Export from alix
        let exported = export_contacts(&alix.db())?;

        // Import - should update because alix's contact is newer
        let imported_count = import_contacts(&alix2.db(), &exported)?;
        assert_eq!(imported_count, 1);

        // Verify the contact was updated
        let contact = alix2.db().get_contact("inbox_update_test")?.unwrap();
        assert_eq!(contact.display_name, Some("Updated Name".to_string()));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_contact_import_no_update_if_older() {
        tester!(alix);
        tester!(alix2, from: alix);

        // Create a contact on alix first (older)
        let contact_data_old = ContactData {
            display_name: Some("Older Name".to_string()),
            ..Default::default()
        };
        alix.db()
            .add_contact("inbox_no_update_test", contact_data_old)?;

        // Export from alix
        let exported = export_contacts(&alix.db())?;

        // Wait a bit to ensure timestamp difference
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Create a contact on alix2 (newer)
        let contact_data_new = ContactData {
            display_name: Some("Newer Name".to_string()),
            ..Default::default()
        };
        alix2
            .db()
            .add_contact("inbox_no_update_test", contact_data_new)?;

        // Import - should NOT update because alix2's contact is newer
        let imported_count = import_contacts(&alix2.db(), &exported)?;
        assert_eq!(imported_count, 1);

        // Verify the contact was NOT updated (still has newer name)
        let contact = alix2.db().get_contact("inbox_no_update_test")?.unwrap();
        assert_eq!(contact.display_name, Some("Newer Name".to_string()));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_empty_contacts_export() {
        tester!(alix);

        // Export with no contacts
        let exported = export_contacts(&alix.db())?;

        // Should still be valid (just empty)
        let contacts_save = ContactsSave::decode(exported.as_slice())?;
        assert!(contacts_save.contacts.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_multiple_contacts_export_import() {
        tester!(alix);

        // Create multiple contacts
        for i in 0..5 {
            let contact_data = ContactData {
                display_name: Some(format!("User {}", i)),
                ..Default::default()
            };
            alix.db()
                .add_contact(&format!("inbox_{}", i), contact_data)?;
        }

        // Export
        let exported = export_contacts(&alix.db())?;

        // Import to new installation
        tester!(alix2, from: alix);
        let imported_count = import_contacts(&alix2.db(), &exported)?;
        assert_eq!(imported_count, 5);

        // Verify all contacts exist
        let contacts = alix2.db().get_contacts(None)?;
        assert_eq!(contacts.len(), 5);
    }
}
