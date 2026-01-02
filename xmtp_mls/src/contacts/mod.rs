use xmtp_db::{
    StorageError,
    encrypted_store::contacts::{
        AddressData, ContactData, FullContact, QueryContacts, StoredContact,
    },
};

use crate::context::XmtpSharedContext;

mod address;
mod client;
mod email;
mod phone_number;
mod url;
mod wallet_address;

pub use address::ContactAddress;
pub use email::ContactEmail;
pub use phone_number::ContactPhoneNumber;
pub use url::ContactUrl;
pub use wallet_address::ContactWalletAddress;

pub struct Contact<Context> {
    context: Context,
    pub inbox_id: String,
    pub display_name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub company: Option<String>,
    pub job_title: Option<String>,
    pub birthday: Option<String>,
    pub note: Option<String>,
    pub image_url: Option<String>,
    pub is_favorite: bool,
    pub created_at_ns: i64,
    pub updated_at_ns: i64,
}

impl<Context: XmtpSharedContext + Clone> Contact<Context> {
    pub fn new(context: Context, c: FullContact) -> Self {
        Self {
            context,
            inbox_id: c.inbox_id,
            display_name: c.display_name,
            first_name: c.first_name,
            last_name: c.last_name,
            prefix: c.prefix,
            suffix: c.suffix,
            company: c.company,
            job_title: c.job_title,
            birthday: c.birthday,
            note: c.note,
            image_url: c.image_url,
            is_favorite: c.is_favorite,
            created_at_ns: c.created_at_ns,
            updated_at_ns: c.updated_at_ns,
        }
    }

    pub fn from_stored(context: Context, c: StoredContact) -> Self {
        Self {
            context,
            inbox_id: c.inbox_id,
            display_name: c.display_name,
            first_name: c.first_name,
            last_name: c.last_name,
            prefix: c.prefix,
            suffix: c.suffix,
            company: c.company,
            job_title: c.job_title,
            birthday: c.birthday,
            note: c.note,
            image_url: c.image_url,
            is_favorite: c.is_favorite != 0,
            created_at_ns: c.created_at_ns,
            updated_at_ns: c.updated_at_ns,
        }
    }

    pub fn update(&self, data: ContactData) -> Result<(), StorageError> {
        self.context.db().update_contact(&self.inbox_id, data)
    }

    pub fn delete(&self) -> Result<(), StorageError> {
        self.context.db().delete_contact(&self.inbox_id)
    }

    pub fn phone_numbers(&self) -> Result<Vec<ContactPhoneNumber<Context>>, StorageError> {
        let phone_numbers = self.context.db().get_phone_numbers(&self.inbox_id)?;
        Ok(phone_numbers
            .into_iter()
            .map(|p| ContactPhoneNumber::new(self.context.clone(), p))
            .collect())
    }

    pub fn add_phone_number(
        &self,
        phone_number: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        self.context
            .db()
            .add_phone_number(&self.inbox_id, phone_number, label)?;
        Ok(())
    }

    pub fn emails(&self) -> Result<Vec<ContactEmail<Context>>, StorageError> {
        let emails = self.context.db().get_emails(&self.inbox_id)?;
        Ok(emails
            .into_iter()
            .map(|e| ContactEmail::new(self.context.clone(), e))
            .collect())
    }

    pub fn add_email(&self, email: String, label: Option<String>) -> Result<(), StorageError> {
        self.context.db().add_email(&self.inbox_id, email, label)?;
        Ok(())
    }

    pub fn urls(&self) -> Result<Vec<ContactUrl<Context>>, StorageError> {
        let urls = self.context.db().get_urls(&self.inbox_id)?;
        Ok(urls
            .into_iter()
            .map(|u| ContactUrl::new(self.context.clone(), u))
            .collect())
    }

    pub fn add_url(&self, url: String, label: Option<String>) -> Result<(), StorageError> {
        self.context.db().add_url(&self.inbox_id, url, label)?;
        Ok(())
    }

    pub fn wallet_addresses(&self) -> Result<Vec<ContactWalletAddress<Context>>, StorageError> {
        let wallet_addresses = self.context.db().get_wallet_addresses(&self.inbox_id)?;
        Ok(wallet_addresses
            .into_iter()
            .map(|w| ContactWalletAddress::new(self.context.clone(), w))
            .collect())
    }

    pub fn add_wallet_address(
        &self,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        self.context
            .db()
            .add_wallet_address(&self.inbox_id, wallet_address, label)?;
        Ok(())
    }

    pub fn addresses(&self) -> Result<Vec<ContactAddress<Context>>, StorageError> {
        let addresses = self.context.db().get_addresses(&self.inbox_id)?;
        Ok(addresses
            .into_iter()
            .filter_map(|s| ContactAddress::new(self.context.clone(), s))
            .collect())
    }

    pub fn add_address(&self, data: AddressData) -> Result<(), StorageError> {
        self.context.db().add_address(&self.inbox_id, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ClientBuilder;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[xmtp_common::test]
    async fn test_client_create_and_get_contact() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let data = ContactData {
            display_name: Some("Alice".to_string()),
            first_name: Some("Alice".to_string()),
            last_name: Some("Smith".to_string()),
            ..Default::default()
        };

        let contact = client.create_contact("inbox_alice", data).unwrap();
        assert_eq!(contact.inbox_id, "inbox_alice");
        assert_eq!(contact.display_name, Some("Alice".to_string()));
        assert_eq!(contact.first_name, Some("Alice".to_string()));

        let retrieved = client.get_contact("inbox_alice").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.inbox_id, "inbox_alice");
        assert_eq!(retrieved.display_name, Some("Alice".to_string()));
    }

    #[xmtp_common::test]
    async fn test_client_list_contacts() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        client
            .create_contact(
                "inbox_1",
                ContactData {
                    display_name: Some("User 1".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        client
            .create_contact(
                "inbox_2",
                ContactData {
                    display_name: Some("User 2".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        let contacts = client.list_contacts(None).unwrap();
        assert_eq!(contacts.len(), 2);
    }

    #[xmtp_common::test]
    async fn test_contact_update() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_update",
                ContactData {
                    display_name: Some("Original".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .update(ContactData {
                display_name: Some("Updated".to_string()),
                is_favorite: Some(true),
                ..Default::default()
            })
            .unwrap();

        let updated = client.get_contact("inbox_update").unwrap().unwrap();
        assert_eq!(updated.display_name, Some("Updated".to_string()));
        assert!(updated.is_favorite);
    }

    #[xmtp_common::test]
    async fn test_contact_delete() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_delete",
                ContactData {
                    display_name: Some("To Delete".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        assert!(client.get_contact("inbox_delete").unwrap().is_some());

        contact.delete().unwrap();

        assert!(client.get_contact("inbox_delete").unwrap().is_none());
    }

    #[xmtp_common::test]
    async fn test_contact_phone_numbers_lazy_loading() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_phones",
                ContactData {
                    display_name: Some("Phone Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        // Initially empty
        let phones = contact.phone_numbers().unwrap();
        assert!(phones.is_empty());

        // Add phone numbers
        contact
            .add_phone_number("555-1234".to_string(), Some("Mobile".to_string()))
            .unwrap();
        contact
            .add_phone_number("555-5678".to_string(), Some("Work".to_string()))
            .unwrap();

        // Now should have 2 phone numbers
        let phones = contact.phone_numbers().unwrap();
        assert_eq!(phones.len(), 2);
        assert_eq!(phones[0].phone_number, "555-1234");
        assert_eq!(phones[0].label, Some("Mobile".to_string()));
    }

    #[xmtp_common::test]
    async fn test_contact_emails_lazy_loading() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_emails",
                ContactData {
                    display_name: Some("Email Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_email("test@example.com".to_string(), Some("Personal".to_string()))
            .unwrap();

        let emails = contact.emails().unwrap();
        assert_eq!(emails.len(), 1);
        assert_eq!(emails[0].email, "test@example.com");
        assert_eq!(emails[0].label, Some("Personal".to_string()));
    }

    #[xmtp_common::test]
    async fn test_contact_urls_lazy_loading() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_urls",
                ContactData {
                    display_name: Some("URL Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_url(
                "https://example.com".to_string(),
                Some("Website".to_string()),
            )
            .unwrap();

        let urls = contact.urls().unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://example.com");
    }

    #[xmtp_common::test]
    async fn test_contact_wallet_addresses_lazy_loading() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_wallets",
                ContactData {
                    display_name: Some("Wallet Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_wallet_address("0x1234567890abcdef".to_string(), Some("Main".to_string()))
            .unwrap();

        let wallets = contact.wallet_addresses().unwrap();
        assert_eq!(wallets.len(), 1);
        assert_eq!(wallets[0].wallet_address, "0x1234567890abcdef");
    }

    #[xmtp_common::test]
    async fn test_contact_addresses_lazy_loading() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_addrs",
                ContactData {
                    display_name: Some("Address Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_address(AddressData {
                address1: Some("123 Main St".to_string()),
                city: Some("Springfield".to_string()),
                region: Some("IL".to_string()),
                postal_code: Some("62701".to_string()),
                country: Some("USA".to_string()),
                label: Some("Home".to_string()),
                ..Default::default()
            })
            .unwrap();

        let addrs = contact.addresses().unwrap();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].address1, Some("123 Main St".to_string()));
        assert_eq!(addrs[0].city, Some("Springfield".to_string()));
    }

    #[xmtp_common::test]
    async fn test_phone_number_update() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_phone_update",
                ContactData {
                    display_name: Some("Phone Update Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_phone_number("555-0000".to_string(), Some("Old".to_string()))
            .unwrap();

        let phones = contact.phone_numbers().unwrap();
        assert_eq!(phones.len(), 1);

        phones[0]
            .update("555-9999".to_string(), Some("New".to_string()))
            .unwrap();

        let updated_phones = contact.phone_numbers().unwrap();
        assert_eq!(updated_phones[0].phone_number, "555-9999");
        assert_eq!(updated_phones[0].label, Some("New".to_string()));
    }

    #[xmtp_common::test]
    async fn test_phone_number_delete() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_phone_delete",
                ContactData {
                    display_name: Some("Phone Delete Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_phone_number("555-1111".to_string(), None)
            .unwrap();

        let phones = contact.phone_numbers().unwrap();
        assert_eq!(phones.len(), 1);

        phones[0].delete().unwrap();

        let phones = contact.phone_numbers().unwrap();
        assert!(phones.is_empty());
    }

    #[xmtp_common::test]
    async fn test_email_update_and_delete() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_email_ops",
                ContactData {
                    display_name: Some("Email Ops Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_email("old@example.com".to_string(), Some("Old".to_string()))
            .unwrap();

        let emails = contact.emails().unwrap();
        emails[0]
            .update("new@example.com".to_string(), Some("New".to_string()))
            .unwrap();

        let updated = contact.emails().unwrap();
        assert_eq!(updated[0].email, "new@example.com");

        updated[0].delete().unwrap();
        assert!(contact.emails().unwrap().is_empty());
    }

    #[xmtp_common::test]
    async fn test_url_update_and_delete() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_url_ops",
                ContactData {
                    display_name: Some("URL Ops Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_url("https://old.com".to_string(), None)
            .unwrap();

        let urls = contact.urls().unwrap();
        urls[0]
            .update("https://new.com".to_string(), Some("Updated".to_string()))
            .unwrap();

        let updated = contact.urls().unwrap();
        assert_eq!(updated[0].url, "https://new.com");

        updated[0].delete().unwrap();
        assert!(contact.urls().unwrap().is_empty());
    }

    #[xmtp_common::test]
    async fn test_wallet_address_update_and_delete() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_wallet_ops",
                ContactData {
                    display_name: Some("Wallet Ops Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_wallet_address("0xold".to_string(), None)
            .unwrap();

        let wallets = contact.wallet_addresses().unwrap();
        wallets[0]
            .update("0xnew".to_string(), Some("Updated".to_string()))
            .unwrap();

        let updated = contact.wallet_addresses().unwrap();
        assert_eq!(updated[0].wallet_address, "0xnew");

        updated[0].delete().unwrap();
        assert!(contact.wallet_addresses().unwrap().is_empty());
    }

    #[xmtp_common::test]
    async fn test_address_update_and_delete() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let contact = client
            .create_contact(
                "inbox_addr_ops",
                ContactData {
                    display_name: Some("Address Ops Test".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        contact
            .add_address(AddressData {
                address1: Some("Old Street".to_string()),
                city: Some("Old City".to_string()),
                ..Default::default()
            })
            .unwrap();

        let addrs = contact.addresses().unwrap();
        addrs[0]
            .update(AddressData {
                address1: Some("New Street".to_string()),
                city: Some("New City".to_string()),
                ..Default::default()
            })
            .unwrap();

        let updated = contact.addresses().unwrap();
        assert_eq!(updated[0].address1, Some("New Street".to_string()));
        assert_eq!(updated[0].city, Some("New City".to_string()));

        updated[0].delete().unwrap();
        assert!(contact.addresses().unwrap().is_empty());
    }
}
