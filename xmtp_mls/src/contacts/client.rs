use super::Contact;
use crate::{Client, client::ClientError, context::XmtpSharedContext};
use xmtp_db::encrypted_store::contacts::{ContactData, ContactsQuery, QueryContacts};

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + Clone,
{
    /// Get a contact by inbox ID.
    pub fn get_contact(&self, inbox_id: &str) -> Result<Option<Contact<Context>>, ClientError> {
        let contact = self.context.db().get_contact(inbox_id)?;
        Ok(contact.map(|c| Contact::new(self.context.clone(), c)))
    }

    /// List contacts with optional filtering, search, and pagination.
    pub fn list_contacts(
        &self,
        query: Option<ContactsQuery>,
    ) -> Result<Vec<Contact<Context>>, ClientError> {
        let contacts = self.context.db().get_contacts(query)?;
        Ok(contacts
            .into_iter()
            .map(|c| Contact::new(self.context.clone(), c))
            .collect())
    }

    /// Create a new contact.
    pub fn create_contact(
        &self,
        inbox_id: &str,
        data: ContactData,
    ) -> Result<Contact<Context>, ClientError> {
        let stored = self.context.db().add_contact(inbox_id, data)?;
        Ok(Contact::from_stored(self.context.clone(), stored))
    }
}
