mod emails;
mod phone_numbers;
mod addresses;
mod urls;
mod wallet_addresses;

pub use emails::*;
pub use phone_numbers::*;
pub use addresses::*;
pub use urls::*;
pub use wallet_addresses::*;
use xmtp_proto::mls_v1::SortDirection;

use super::ConnectionExt;
use super::db_connection::DbConnection;
use super::schema::contacts::{self, dsl};
use super::schema::{
    contact_emails, contact_phone_numbers, contact_addresses, contact_urls,
    contact_wallet_addresses,
};
use crate::StorageError;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Integer, Nullable, Text};
use serde::{Deserialize, Serialize};

/// StoredContact represents a contact in the database
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Selectable,
    PartialEq,
    Eq,
)]
#[diesel(table_name = contacts)]
#[diesel(primary_key(id))]
pub struct StoredContact {
    pub id: i32,
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
    pub is_favorite: i32,
    pub created_at_ns: i64,
    pub updated_at_ns: i64,
}

/// Contact data for creating or updating contacts (without id, inbox_id, or timestamps)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactData {
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
    pub is_favorite: Option<bool>,
}

/// Internal struct for inserting contacts with timestamps
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = contacts)]
struct InsertableContact {
    inbox_id: String,
    display_name: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    company: Option<String>,
    job_title: Option<String>,
    birthday: Option<String>,
    note: Option<String>,
    image_url: Option<String>,
    is_favorite: i32,
    created_at_ns: i64,
    updated_at_ns: i64,
}

impl InsertableContact {
    fn new(inbox_id: String, data: ContactData) -> Self {
        let now = xmtp_common::time::now_ns();
        Self {
            inbox_id,
            display_name: data.display_name,
            first_name: data.first_name,
            last_name: data.last_name,
            prefix: data.prefix,
            suffix: data.suffix,
            company: data.company,
            job_title: data.job_title,
            birthday: data.birthday,
            note: data.note,
            image_url: data.image_url,
            is_favorite: data.is_favorite.map(|b| b as i32).unwrap_or(0),
            created_at_ns: now,
            updated_at_ns: now,
        }
    }
}

/// Internal struct for updating contacts (None fields are not updated)
#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = contacts)]
struct UpdatableContact {
    display_name: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    company: Option<String>,
    job_title: Option<String>,
    birthday: Option<String>,
    note: Option<String>,
    image_url: Option<String>,
    is_favorite: Option<i32>,
    updated_at_ns: i64,
}

impl UpdatableContact {
    fn from_data(data: ContactData) -> Self {
        Self {
            display_name: data.display_name,
            first_name: data.first_name,
            last_name: data.last_name,
            prefix: data.prefix,
            suffix: data.suffix,
            company: data.company,
            job_title: data.job_title,
            birthday: data.birthday,
            note: data.note,
            image_url: data.image_url,
            is_favorite: data.is_favorite.map(|b| b as i32),
            updated_at_ns: xmtp_common::time::now_ns(),
        }
    }
}

/// Phone number data for FullContact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhoneNumber {
    pub id: i32,
    pub phone_number: String,
    pub label: Option<String>,
}

/// Email data for FullContact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Email {
    pub id: i32,
    pub email: String,
    pub label: Option<String>,
}

/// URL data for FullContact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Url {
    pub id: i32,
    pub url: String,
    pub label: Option<String>,
}

/// Wallet address data for FullContact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletAddress {
    pub id: i32,
    pub wallet_address: String,
    pub label: Option<String>,
}

/// Raw struct for querying the contact_list view
#[derive(Debug, Clone, QueryableByName)]
struct RawFullContact {
    #[diesel(sql_type = Text)]
    inbox_id: String,
    #[diesel(sql_type = Nullable<Text>)]
    display_name: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    first_name: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    last_name: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    prefix: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    suffix: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    company: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    job_title: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    birthday: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    note: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    image_url: Option<String>,
    #[diesel(sql_type = Integer)]
    is_favorite: i32,
    #[diesel(sql_type = BigInt)]
    created_at_ns: i64,
    #[diesel(sql_type = BigInt)]
    updated_at_ns: i64,
    #[diesel(sql_type = Text)]
    phone_numbers: String,
    #[diesel(sql_type = Text)]
    emails: String,
    #[diesel(sql_type = Text)]
    urls: String,
    #[diesel(sql_type = Text)]
    wallet_addresses: String,
    #[diesel(sql_type = Text)]
    addresses: String,
}

/// A complete contact with all related data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FullContact {
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
    pub phone_numbers: Vec<PhoneNumber>,
    pub emails: Vec<Email>,
    pub urls: Vec<Url>,
    pub wallet_addresses: Vec<WalletAddress>,
    pub addresses: Vec<AddressData>,
}

impl From<RawFullContact> for FullContact {
    fn from(raw: RawFullContact) -> Self {
        Self {
            inbox_id: raw.inbox_id,
            display_name: raw.display_name,
            first_name: raw.first_name,
            last_name: raw.last_name,
            prefix: raw.prefix,
            suffix: raw.suffix,
            company: raw.company,
            job_title: raw.job_title,
            birthday: raw.birthday,
            note: raw.note,
            image_url: raw.image_url,
            is_favorite: raw.is_favorite != 0,
            created_at_ns: raw.created_at_ns,
            updated_at_ns: raw.updated_at_ns,
            phone_numbers: serde_json::from_str(&raw.phone_numbers).unwrap_or_default(),
            emails: serde_json::from_str(&raw.emails).unwrap_or_default(),
            urls: serde_json::from_str(&raw.urls).unwrap_or_default(),
            wallet_addresses: serde_json::from_str(&raw.wallet_addresses).unwrap_or_default(),
            addresses: serde_json::from_str(&raw.addresses).unwrap_or_default(),
        }
    }
}

/// Sort field for contacts query
#[derive(Debug, Clone, Copy, Default)]
pub enum ContactSortField {
    #[default]
    DisplayName,
    FirstName,
    LastName,
    InboxId,
    CreatedAt,
    UpdatedAt,
}

/// Query parameters for filtering, searching, and paginating contacts
#[derive(Debug, Clone, Default)]
pub struct ContactsQuery {
    /// Text search across name fields (display_name, first_name, last_name)
    pub search: Option<String>,
    /// Filter by favorite status
    pub is_favorite: Option<bool>,
    /// Field to sort by
    pub sort_by: Option<ContactSortField>,
    /// Sort direction
    pub sort_direction: Option<SortDirection>,
    /// Maximum number of results to return
    pub limit: Option<i64>,
    /// Number of results to skip
    pub offset: Option<i64>,
}

pub trait QueryContacts {
    /// Add a new contact (timestamps set automatically)
    fn add_contact(&self, inbox_id: &str, data: ContactData)
    -> Result<StoredContact, StorageError>;

    /// Update an existing contact (updated_at_ns set automatically)
    fn update_contact(&self, inbox_id: &str, data: ContactData) -> Result<(), StorageError>;

    /// Get a contact with all related data by inbox_id
    fn get_contact(&self, inbox_id: &str) -> Result<Option<FullContact>, StorageError>;

    /// Get contacts with optional filtering, search, and pagination
    fn get_contacts(&self, query: Option<ContactsQuery>) -> Result<Vec<FullContact>, StorageError>;

    /// Delete a contact and all related data by inbox_id
    fn delete_contact(&self, inbox_id: &str) -> Result<(), StorageError>;

    // Phone number operations
    fn get_phone_numbers(&self, inbox_id: &str) -> Result<Vec<PhoneNumber>, StorageError>;
    fn add_phone_number(
        &self,
        inbox_id: &str,
        phone_number: String,
        label: Option<String>,
    ) -> Result<StoredContactPhoneNumber, StorageError>;
    fn update_phone_number(
        &self,
        id: i32,
        phone_number: String,
        label: Option<String>,
    ) -> Result<(), StorageError>;
    fn delete_phone_number(&self, id: i32) -> Result<(), StorageError>;

    // Email operations
    fn get_emails(&self, inbox_id: &str) -> Result<Vec<Email>, StorageError>;
    fn add_email(
        &self,
        inbox_id: &str,
        email: String,
        label: Option<String>,
    ) -> Result<StoredContactEmail, StorageError>;
    fn update_email(
        &self,
        id: i32,
        email: String,
        label: Option<String>,
    ) -> Result<(), StorageError>;
    fn delete_email(&self, id: i32) -> Result<(), StorageError>;

    // URL operations
    fn get_urls(&self, inbox_id: &str) -> Result<Vec<Url>, StorageError>;
    fn add_url(
        &self,
        inbox_id: &str,
        url: String,
        label: Option<String>,
    ) -> Result<StoredContactUrl, StorageError>;
    fn update_url(&self, id: i32, url: String, label: Option<String>) -> Result<(), StorageError>;
    fn delete_url(&self, id: i32) -> Result<(), StorageError>;

    // Wallet address operations
    fn get_wallet_addresses(&self, inbox_id: &str) -> Result<Vec<WalletAddress>, StorageError>;
    fn add_wallet_address(
        &self,
        inbox_id: &str,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<StoredContactWalletAddress, StorageError>;
    fn update_wallet_address(
        &self,
        id: i32,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<(), StorageError>;
    fn delete_wallet_address(&self, id: i32) -> Result<(), StorageError>;

    // Street address operations
    fn get_addresses(&self, inbox_id: &str) -> Result<Vec<AddressData>, StorageError>;
    fn add_address(
        &self,
        inbox_id: &str,
        data: AddressData,
    ) -> Result<StoredContactAddress, StorageError>;
    fn update_address(&self, id: i32, data: AddressData) -> Result<(), StorageError>;
    fn delete_address(&self, id: i32) -> Result<(), StorageError>;
}

impl<C: ConnectionExt> QueryContacts for DbConnection<C> {
    fn add_contact(
        &self,
        inbox_id: &str,
        data: ContactData,
    ) -> Result<StoredContact, StorageError> {
        let insertable = InsertableContact::new(inbox_id.to_string(), data);
        Ok(self.raw_query_write(|conn| {
            diesel::insert_into(contacts::table)
                .values(&insertable)
                .execute(conn)?;
            dsl::contacts.order(dsl::id.desc()).first(conn)
        })?)
    }

    fn update_contact(&self, inbox_id: &str, data: ContactData) -> Result<(), StorageError> {
        let updatable = UpdatableContact::from_data(data);
        self.raw_query_write(|conn| {
            diesel::update(dsl::contacts.filter(dsl::inbox_id.eq(inbox_id)))
                .set(&updatable)
                .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn get_contact(&self, inbox_id: &str) -> Result<Option<FullContact>, StorageError> {
        Ok(self.raw_query_read(|conn| {
            let result: Option<RawFullContact> =
                diesel::sql_query("SELECT * FROM contact_list WHERE inbox_id = ?")
                    .bind::<Text, _>(inbox_id)
                    .get_result(conn)
                    .optional()?;
            Ok(result.map(FullContact::from))
        })?)
    }

    fn get_contacts(&self, query: Option<ContactsQuery>) -> Result<Vec<FullContact>, StorageError> {
        Ok(self.raw_query_read(|conn| {
            let query = query.unwrap_or_default();

            // Build the query based on whether we have a search term
            // With search: JOIN with FTS5 table for fast indexed search
            // Without search: query contact_list directly
            let mut sql = if query.search.is_some() {
                String::from(
                    "SELECT cl.* FROM contact_list cl \
                     JOIN contacts_fts fts ON fts.inbox_id = cl.inbox_id \
                     WHERE contacts_fts MATCH ?",
                )
            } else {
                String::from("SELECT * FROM contact_list WHERE 1=1")
            };

            // Build FTS search pattern if searching
            let search_pattern = query.search.as_ref().map(|search| {
                // FTS5 trigram tokenizer uses quoted string for substring match
                // Escape any quotes in the search term
                let escaped = search.replace('"', "\"\"");
                format!("\"{}\"", escaped)
            });

            // is_favorite filter (bool-derived, safe for interpolation)
            if let Some(is_fav) = query.is_favorite {
                sql.push_str(&format!(
                    " AND is_favorite = {}",
                    if is_fav { 1 } else { 0 }
                ));
            }

            // Sorting (enum-controlled, safe for interpolation)
            let sort_field = match query.sort_by.unwrap_or_default() {
                ContactSortField::DisplayName => "display_name",
                ContactSortField::FirstName => "first_name",
                ContactSortField::LastName => "last_name",
                ContactSortField::InboxId => "inbox_id",
                ContactSortField::CreatedAt => "created_at_ns",
                ContactSortField::UpdatedAt => "updated_at_ns",
            };
            let sort_dir = match query.sort_direction.unwrap_or_default() {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
                SortDirection::Unspecified => "ASC",
            };
            sql.push_str(&format!(" ORDER BY {} {}", sort_field, sort_dir));

            // Pagination (i64 values, safe for interpolation)
            if let Some(limit) = query.limit {
                sql.push_str(&format!(" LIMIT {}", limit));
            }
            if let Some(offset) = query.offset {
                sql.push_str(&format!(" OFFSET {}", offset));
            }

            // Execute query - only search requires binding (user-provided string)
            let results: Vec<RawFullContact> = match &search_pattern {
                None => diesel::sql_query(&sql).load(conn)?,
                Some(s) => diesel::sql_query(&sql).bind::<Text, _>(s).load(conn)?,
            };

            Ok(results.into_iter().map(FullContact::from).collect())
        })?)
    }

    fn delete_contact(&self, inbox_id: &str) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(dsl::contacts.filter(dsl::inbox_id.eq(inbox_id))).execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn get_phone_numbers(&self, inbox_id: &str) -> Result<Vec<PhoneNumber>, StorageError> {
        Ok(self.raw_query_read(|conn| {
            let contact: Option<StoredContact> = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)
                .optional()?;
            let Some(contact) = contact else {
                return Ok(Vec::new());
            };
            let phone_numbers: Vec<StoredContactPhoneNumber> =
                contact_phone_numbers::dsl::contact_phone_numbers
                    .filter(contact_phone_numbers::dsl::contact_id.eq(contact.id))
                    .load(conn)?;
            Ok(phone_numbers
                .into_iter()
                .map(|p| PhoneNumber {
                    id: p.id,
                    phone_number: p.phone_number,
                    label: p.label,
                })
                .collect())
        })?)
    }

    fn add_phone_number(
        &self,
        inbox_id: &str,
        phone_number: String,
        label: Option<String>,
    ) -> Result<StoredContactPhoneNumber, StorageError> {
        Ok(self.raw_query_write(|conn| {
            let contact: StoredContact = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)?;
            let new = NewContactPhoneNumber {
                contact_id: contact.id,
                phone_number,
                label,
            };
            diesel::insert_into(contact_phone_numbers::table)
                .values(&new)
                .execute(conn)?;
            contact_phone_numbers::dsl::contact_phone_numbers
                .order(contact_phone_numbers::dsl::id.desc())
                .first(conn)
        })?)
    }

    fn update_phone_number(
        &self,
        id: i32,
        phone_number: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(
                contact_phone_numbers::dsl::contact_phone_numbers
                    .filter(contact_phone_numbers::dsl::id.eq(id)),
            )
            .set((
                contact_phone_numbers::dsl::phone_number.eq(phone_number),
                contact_phone_numbers::dsl::label.eq(label),
            ))
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn delete_phone_number(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(
                contact_phone_numbers::dsl::contact_phone_numbers
                    .filter(contact_phone_numbers::dsl::id.eq(id)),
            )
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn get_emails(&self, inbox_id: &str) -> Result<Vec<Email>, StorageError> {
        Ok(self.raw_query_read(|conn| {
            let contact: Option<StoredContact> = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)
                .optional()?;
            let Some(contact) = contact else {
                return Ok(Vec::new());
            };
            let emails: Vec<StoredContactEmail> = contact_emails::dsl::contact_emails
                .filter(contact_emails::dsl::contact_id.eq(contact.id))
                .load(conn)?;
            Ok(emails
                .into_iter()
                .map(|e| Email {
                    id: e.id,
                    email: e.email,
                    label: e.label,
                })
                .collect())
        })?)
    }

    fn add_email(
        &self,
        inbox_id: &str,
        email: String,
        label: Option<String>,
    ) -> Result<StoredContactEmail, StorageError> {
        Ok(self.raw_query_write(|conn| {
            let contact: StoredContact = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)?;
            let new = NewContactEmail {
                contact_id: contact.id,
                email,
                label,
            };
            diesel::insert_into(contact_emails::table)
                .values(&new)
                .execute(conn)?;
            contact_emails::dsl::contact_emails
                .order(contact_emails::dsl::id.desc())
                .first(conn)
        })?)
    }

    fn update_email(
        &self,
        id: i32,
        email: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(
                contact_emails::dsl::contact_emails.filter(contact_emails::dsl::id.eq(id)),
            )
            .set((
                contact_emails::dsl::email.eq(email),
                contact_emails::dsl::label.eq(label),
            ))
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn delete_email(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(
                contact_emails::dsl::contact_emails.filter(contact_emails::dsl::id.eq(id)),
            )
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn get_urls(&self, inbox_id: &str) -> Result<Vec<Url>, StorageError> {
        Ok(self.raw_query_read(|conn| {
            let contact: Option<StoredContact> = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)
                .optional()?;
            let Some(contact) = contact else {
                return Ok(Vec::new());
            };
            let urls: Vec<StoredContactUrl> = contact_urls::dsl::contact_urls
                .filter(contact_urls::dsl::contact_id.eq(contact.id))
                .load(conn)?;
            Ok(urls
                .into_iter()
                .map(|u| Url {
                    id: u.id,
                    url: u.url,
                    label: u.label,
                })
                .collect())
        })?)
    }

    fn add_url(
        &self,
        inbox_id: &str,
        url: String,
        label: Option<String>,
    ) -> Result<StoredContactUrl, StorageError> {
        Ok(self.raw_query_write(|conn| {
            let contact: StoredContact = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)?;
            let new = NewContactUrl {
                contact_id: contact.id,
                url,
                label,
            };
            diesel::insert_into(contact_urls::table)
                .values(&new)
                .execute(conn)?;
            contact_urls::dsl::contact_urls
                .order(contact_urls::dsl::id.desc())
                .first(conn)
        })?)
    }

    fn update_url(&self, id: i32, url: String, label: Option<String>) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(contact_urls::dsl::contact_urls.filter(contact_urls::dsl::id.eq(id)))
                .set((
                    contact_urls::dsl::url.eq(url),
                    contact_urls::dsl::label.eq(label),
                ))
                .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn delete_url(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(contact_urls::dsl::contact_urls.filter(contact_urls::dsl::id.eq(id)))
                .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn get_wallet_addresses(&self, inbox_id: &str) -> Result<Vec<WalletAddress>, StorageError> {
        Ok(self.raw_query_read(|conn| {
            let contact: Option<StoredContact> = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)
                .optional()?;
            let Some(contact) = contact else {
                return Ok(Vec::new());
            };
            let wallet_addresses: Vec<StoredContactWalletAddress> =
                contact_wallet_addresses::dsl::contact_wallet_addresses
                    .filter(contact_wallet_addresses::dsl::contact_id.eq(contact.id))
                    .load(conn)?;
            Ok(wallet_addresses
                .into_iter()
                .map(|w| WalletAddress {
                    id: w.id,
                    wallet_address: w.wallet_address,
                    label: w.label,
                })
                .collect())
        })?)
    }

    fn add_wallet_address(
        &self,
        inbox_id: &str,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<StoredContactWalletAddress, StorageError> {
        Ok(self.raw_query_write(|conn| {
            let contact: StoredContact = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)?;
            let new = NewContactWalletAddress {
                contact_id: contact.id,
                wallet_address,
                label,
            };
            diesel::insert_into(contact_wallet_addresses::table)
                .values(&new)
                .execute(conn)?;
            contact_wallet_addresses::dsl::contact_wallet_addresses
                .order(contact_wallet_addresses::dsl::id.desc())
                .first(conn)
        })?)
    }

    fn update_wallet_address(
        &self,
        id: i32,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(
                contact_wallet_addresses::dsl::contact_wallet_addresses
                    .filter(contact_wallet_addresses::dsl::id.eq(id)),
            )
            .set((
                contact_wallet_addresses::dsl::wallet_address.eq(wallet_address),
                contact_wallet_addresses::dsl::label.eq(label),
            ))
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn delete_wallet_address(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(
                contact_wallet_addresses::dsl::contact_wallet_addresses
                    .filter(contact_wallet_addresses::dsl::id.eq(id)),
            )
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn get_addresses(&self, inbox_id: &str) -> Result<Vec<AddressData>, StorageError> {
        Ok(self.raw_query_read(|conn| {
            let contact: Option<StoredContact> = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)
                .optional()?;
            let Some(contact) = contact else {
                return Ok(Vec::new());
            };
            let addresses: Vec<StoredContactAddress> =
                contact_addresses::dsl::contact_addresses
                    .filter(contact_addresses::dsl::contact_id.eq(contact.id))
                    .load(conn)?;
            Ok(addresses
                .into_iter()
                .map(|s| AddressData {
                    id: Some(s.id),
                    address1: s.address1,
                    address2: s.address2,
                    address3: s.address3,
                    city: s.city,
                    region: s.region,
                    postal_code: s.postal_code,
                    country: s.country,
                    label: s.label,
                })
                .collect())
        })?)
    }

    fn add_address(
        &self,
        inbox_id: &str,
        data: AddressData,
    ) -> Result<StoredContactAddress, StorageError> {
        Ok(self.raw_query_write(|conn| {
            let contact: StoredContact = dsl::contacts
                .filter(dsl::inbox_id.eq(inbox_id))
                .first(conn)?;
            let mut new: NewContactAddress = data.into();
            new.contact_id = contact.id;
            diesel::insert_into(contact_addresses::table)
                .values(&new)
                .execute(conn)?;
            contact_addresses::dsl::contact_addresses
                .order(contact_addresses::dsl::id.desc())
                .first(conn)
        })?)
    }

    fn update_address(&self, id: i32, data: AddressData) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(
                contact_addresses::dsl::contact_addresses
                    .filter(contact_addresses::dsl::id.eq(id)),
            )
            .set((
                contact_addresses::dsl::address1.eq(data.address1),
                contact_addresses::dsl::address2.eq(data.address2),
                contact_addresses::dsl::address3.eq(data.address3),
                contact_addresses::dsl::city.eq(data.city),
                contact_addresses::dsl::region.eq(data.region),
                contact_addresses::dsl::postal_code.eq(data.postal_code),
                contact_addresses::dsl::country.eq(data.country),
                contact_addresses::dsl::label.eq(data.label),
            ))
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    fn delete_address(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(
                contact_addresses::dsl::contact_addresses
                    .filter(contact_addresses::dsl::id.eq(id)),
            )
            .execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }
}

impl<T: QueryContacts + ?Sized> QueryContacts for &T {
    fn add_contact(
        &self,
        inbox_id: &str,
        data: ContactData,
    ) -> Result<StoredContact, StorageError> {
        (**self).add_contact(inbox_id, data)
    }

    fn update_contact(&self, inbox_id: &str, data: ContactData) -> Result<(), StorageError> {
        (**self).update_contact(inbox_id, data)
    }

    fn get_contact(&self, inbox_id: &str) -> Result<Option<FullContact>, StorageError> {
        (**self).get_contact(inbox_id)
    }

    fn get_contacts(&self, query: Option<ContactsQuery>) -> Result<Vec<FullContact>, StorageError> {
        (**self).get_contacts(query)
    }

    fn delete_contact(&self, inbox_id: &str) -> Result<(), StorageError> {
        (**self).delete_contact(inbox_id)
    }

    fn get_phone_numbers(&self, inbox_id: &str) -> Result<Vec<PhoneNumber>, StorageError> {
        (**self).get_phone_numbers(inbox_id)
    }

    fn add_phone_number(
        &self,
        inbox_id: &str,
        phone_number: String,
        label: Option<String>,
    ) -> Result<StoredContactPhoneNumber, StorageError> {
        (**self).add_phone_number(inbox_id, phone_number, label)
    }

    fn update_phone_number(
        &self,
        id: i32,
        phone_number: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        (**self).update_phone_number(id, phone_number, label)
    }

    fn delete_phone_number(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_phone_number(id)
    }

    fn get_emails(&self, inbox_id: &str) -> Result<Vec<Email>, StorageError> {
        (**self).get_emails(inbox_id)
    }

    fn add_email(
        &self,
        inbox_id: &str,
        email: String,
        label: Option<String>,
    ) -> Result<StoredContactEmail, StorageError> {
        (**self).add_email(inbox_id, email, label)
    }

    fn update_email(
        &self,
        id: i32,
        email: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        (**self).update_email(id, email, label)
    }

    fn delete_email(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_email(id)
    }

    fn get_urls(&self, inbox_id: &str) -> Result<Vec<Url>, StorageError> {
        (**self).get_urls(inbox_id)
    }

    fn add_url(
        &self,
        inbox_id: &str,
        url: String,
        label: Option<String>,
    ) -> Result<StoredContactUrl, StorageError> {
        (**self).add_url(inbox_id, url, label)
    }

    fn update_url(&self, id: i32, url: String, label: Option<String>) -> Result<(), StorageError> {
        (**self).update_url(id, url, label)
    }

    fn delete_url(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_url(id)
    }

    fn get_wallet_addresses(&self, inbox_id: &str) -> Result<Vec<WalletAddress>, StorageError> {
        (**self).get_wallet_addresses(inbox_id)
    }

    fn add_wallet_address(
        &self,
        inbox_id: &str,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<StoredContactWalletAddress, StorageError> {
        (**self).add_wallet_address(inbox_id, wallet_address, label)
    }

    fn update_wallet_address(
        &self,
        id: i32,
        wallet_address: String,
        label: Option<String>,
    ) -> Result<(), StorageError> {
        (**self).update_wallet_address(id, wallet_address, label)
    }

    fn delete_wallet_address(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_wallet_address(id)
    }

    fn get_addresses(&self, inbox_id: &str) -> Result<Vec<AddressData>, StorageError> {
        (**self).get_addresses(inbox_id)
    }

    fn add_address(
        &self,
        inbox_id: &str,
        data: AddressData,
    ) -> Result<StoredContactAddress, StorageError> {
        (**self).add_address(inbox_id, data)
    }

    fn update_address(&self, id: i32, data: AddressData) -> Result<(), StorageError> {
        (**self).update_address(id, data)
    }

    fn delete_address(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_address(id)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::with_connection;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn test_add_and_get_contact() {
        with_connection(|conn| {
            let data = ContactData {
                display_name: Some("Alice".to_string()),
                first_name: Some("Alice".to_string()),
                last_name: Some("Smith".to_string()),
                ..Default::default()
            };

            let contact = conn.add_contact("inbox_alice", data)?;
            assert_eq!(contact.inbox_id, "inbox_alice");
            assert_eq!(contact.display_name, Some("Alice".to_string()));
            assert_eq!(contact.first_name, Some("Alice".to_string()));
            assert_eq!(contact.last_name, Some("Smith".to_string()));
            assert_eq!(contact.is_favorite, 0);
            assert!(contact.created_at_ns > 0);
            assert_eq!(contact.created_at_ns, contact.updated_at_ns);

            let retrieved = conn.get_contact("inbox_alice")?;
            assert!(retrieved.is_some());
            let retrieved = retrieved.unwrap();
            assert_eq!(retrieved.inbox_id, "inbox_alice");
            assert_eq!(retrieved.display_name, Some("Alice".to_string()));
            assert!(retrieved.phone_numbers.is_empty());
            assert!(retrieved.emails.is_empty());
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_update_contact() {
        with_connection(|conn| {
            let data = ContactData {
                display_name: Some("Bob".to_string()),
                ..Default::default()
            };
            let contact = conn.add_contact("inbox_bob", data)?;
            let original_created_at = contact.created_at_ns;

            // Small delay to ensure updated_at changes
            std::thread::sleep(std::time::Duration::from_millis(1));

            let update_data = ContactData {
                display_name: Some("Robert".to_string()),
                is_favorite: Some(true),
                ..Default::default()
            };
            conn.update_contact("inbox_bob", update_data)?;

            let updated = conn.get_contact("inbox_bob")?.unwrap();
            assert_eq!(updated.display_name, Some("Robert".to_string()));
            assert!(updated.is_favorite);
            assert_eq!(updated.created_at_ns, original_created_at);
            assert!(updated.updated_at_ns > original_created_at);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_delete_contact() {
        with_connection(|conn| {
            let data = ContactData {
                display_name: Some("Charlie".to_string()),
                ..Default::default()
            };
            conn.add_contact("inbox_charlie", data)?;

            assert!(conn.get_contact("inbox_charlie")?.is_some());

            conn.delete_contact("inbox_charlie")?;

            assert!(conn.get_contact("inbox_charlie")?.is_none());
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_contacts() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_1",
                ContactData {
                    display_name: Some("User 1".to_string()),
                    ..Default::default()
                },
            )?;
            conn.add_contact(
                "inbox_2",
                ContactData {
                    display_name: Some("User 2".to_string()),
                    ..Default::default()
                },
            )?;

            let all = conn.get_contacts(None)?;
            assert_eq!(all.len(), 2);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_phone_numbers() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_phone",
                ContactData {
                    display_name: Some("Phone Test".to_string()),
                    ..Default::default()
                },
            )?;

            let phone = conn.add_phone_number(
                "inbox_phone",
                "555-1234".to_string(),
                Some("Mobile".to_string()),
            )?;
            assert_eq!(phone.phone_number, "555-1234");
            assert_eq!(phone.label, Some("Mobile".to_string()));

            conn.update_phone_number(phone.id, "555-5678".to_string(), Some("Work".to_string()))?;

            let contact = conn.get_contact("inbox_phone")?.unwrap();
            assert_eq!(contact.phone_numbers.len(), 1);
            assert_eq!(contact.phone_numbers[0].phone_number, "555-5678");
            assert_eq!(contact.phone_numbers[0].label, Some("Work".to_string()));

            conn.delete_phone_number(phone.id)?;
            let contact = conn.get_contact("inbox_phone")?.unwrap();
            assert!(contact.phone_numbers.is_empty());
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_emails() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_email",
                ContactData {
                    display_name: Some("Email Test".to_string()),
                    ..Default::default()
                },
            )?;

            let email = conn.add_email(
                "inbox_email",
                "test@example.com".to_string(),
                Some("Personal".to_string()),
            )?;
            assert_eq!(email.email, "test@example.com");

            let contact = conn.get_contact("inbox_email")?.unwrap();
            assert_eq!(contact.emails.len(), 1);

            conn.delete_email(email.id)?;
            let contact = conn.get_contact("inbox_email")?.unwrap();
            assert!(contact.emails.is_empty());
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_addresses() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_addr",
                ContactData {
                    display_name: Some("Address Test".to_string()),
                    ..Default::default()
                },
            )?;

            let addr_data = AddressData {
                address1: Some("123 Main St".to_string()),
                city: Some("Springfield".to_string()),
                region: Some("IL".to_string()),
                postal_code: Some("62701".to_string()),
                country: Some("USA".to_string()),
                label: Some("Home".to_string()),
                ..Default::default()
            };

            let addr = conn.add_address("inbox_addr", addr_data)?;
            assert_eq!(addr.address1, Some("123 Main St".to_string()));
            assert_eq!(addr.city, Some("Springfield".to_string()));

            let contact = conn.get_contact("inbox_addr")?.unwrap();
            assert_eq!(contact.addresses.len(), 1);

            conn.delete_address(addr.id)?;
            let contact = conn.get_contact("inbox_addr")?.unwrap();
            assert!(contact.addresses.is_empty());
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_multiple_companion_entries() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_multi",
                ContactData {
                    display_name: Some("Multi Test".to_string()),
                    ..Default::default()
                },
            )?;

            conn.add_phone_number(
                "inbox_multi",
                "555-1111".to_string(),
                Some("Mobile".to_string()),
            )?;
            conn.add_phone_number(
                "inbox_multi",
                "555-2222".to_string(),
                Some("Work".to_string()),
            )?;
            conn.add_phone_number(
                "inbox_multi",
                "555-3333".to_string(),
                Some("Home".to_string()),
            )?;

            conn.add_email("inbox_multi", "personal@example.com".to_string(), None)?;
            conn.add_email("inbox_multi", "work@example.com".to_string(), None)?;

            let contact = conn.get_contact("inbox_multi")?.unwrap();
            assert_eq!(contact.phone_numbers.len(), 3);
            assert_eq!(contact.emails.len(), 2);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_cascade_delete() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_cascade",
                ContactData {
                    display_name: Some("Cascade Test".to_string()),
                    ..Default::default()
                },
            )?;

            conn.add_phone_number("inbox_cascade", "555-0000".to_string(), None)?;
            conn.add_email("inbox_cascade", "cascade@example.com".to_string(), None)?;

            let contact = conn.get_contact("inbox_cascade")?.unwrap();
            assert_eq!(contact.phone_numbers.len(), 1);
            assert_eq!(contact.emails.len(), 1);

            // Delete contact - should cascade to all companion tables
            conn.delete_contact("inbox_cascade")?;

            assert!(conn.get_contact("inbox_cascade")?.is_none());
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_search_by_name() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_alice",
                ContactData {
                    display_name: Some("Alice Johnson".to_string()),
                    first_name: Some("Alice".to_string()),
                    last_name: Some("Johnson".to_string()),
                    ..Default::default()
                },
            )?;
            conn.add_contact(
                "inbox_bob",
                ContactData {
                    display_name: Some("Bob Smith".to_string()),
                    first_name: Some("Bob".to_string()),
                    last_name: Some("Smith".to_string()),
                    ..Default::default()
                },
            )?;

            // Search by first name
            let results = conn.get_contacts(Some(ContactsQuery {
                search: Some("alice".to_string()),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].inbox_id, "inbox_alice");

            // Search by last name (case insensitive)
            let results = conn.get_contacts(Some(ContactsQuery {
                search: Some("SMITH".to_string()),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].inbox_id, "inbox_bob");
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_search_companion_tables() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_searchable",
                ContactData {
                    display_name: Some("Test User".to_string()),
                    ..Default::default()
                },
            )?;

            // Add companion data
            conn.add_email(
                "inbox_searchable",
                "unique.email@example.org".to_string(),
                None,
            )?;
            conn.add_phone_number("inbox_searchable", "555-UNIQUE".to_string(), None)?;
            conn.add_wallet_address("inbox_searchable", "0xUNIQUEWALLET123".to_string(), None)?;
            conn.add_address(
                "inbox_searchable",
                AddressData {
                    city: Some("UniqueCity".to_string()),
                    ..Default::default()
                },
            )?;

            // Add another contact without these unique values
            conn.add_contact(
                "inbox_other",
                ContactData {
                    display_name: Some("Other User".to_string()),
                    ..Default::default()
                },
            )?;

            // Search by email
            let results = conn.get_contacts(Some(ContactsQuery {
                search: Some("unique.email".to_string()),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].inbox_id, "inbox_searchable");

            // Search by phone number
            let results = conn.get_contacts(Some(ContactsQuery {
                search: Some("555-unique".to_string()),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].inbox_id, "inbox_searchable");

            // Search by wallet address
            let results = conn.get_contacts(Some(ContactsQuery {
                search: Some("uniquewallet".to_string()),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].inbox_id, "inbox_searchable");

            // Search by city in street address
            let results = conn.get_contacts(Some(ContactsQuery {
                search: Some("uniquecity".to_string()),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].inbox_id, "inbox_searchable");
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_filter_by_favorite() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_fav1",
                ContactData {
                    display_name: Some("Favorite 1".to_string()),
                    is_favorite: Some(true),
                    ..Default::default()
                },
            )?;
            conn.add_contact(
                "inbox_fav2",
                ContactData {
                    display_name: Some("Favorite 2".to_string()),
                    is_favorite: Some(true),
                    ..Default::default()
                },
            )?;
            conn.add_contact(
                "inbox_notfav",
                ContactData {
                    display_name: Some("Not Favorite".to_string()),
                    is_favorite: Some(false),
                    ..Default::default()
                },
            )?;

            // Get only favorites
            let results = conn.get_contacts(Some(ContactsQuery {
                is_favorite: Some(true),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 2);
            assert!(results.iter().all(|c| c.is_favorite));

            // Get only non-favorites
            let results = conn.get_contacts(Some(ContactsQuery {
                is_favorite: Some(false),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert!(!results[0].is_favorite);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_pagination() {
        with_connection(|conn| {
            // Add 5 contacts
            for i in 1..=5 {
                conn.add_contact(
                    &format!("inbox_{}", i),
                    ContactData {
                        display_name: Some(format!("User {}", i)),
                        ..Default::default()
                    },
                )?;
            }

            // Get first 2
            let results = conn.get_contacts(Some(ContactsQuery {
                limit: Some(2),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 2);

            // Get next 2 (offset 2)
            let results = conn.get_contacts(Some(ContactsQuery {
                limit: Some(2),
                offset: Some(2),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 2);

            // Get last 1 (offset 4)
            let results = conn.get_contacts(Some(ContactsQuery {
                limit: Some(2),
                offset: Some(4),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_sorting() {
        with_connection(|conn| {
            conn.add_contact(
                "inbox_charlie",
                ContactData {
                    display_name: Some("Charlie".to_string()),
                    first_name: Some("Charlie".to_string()),
                    ..Default::default()
                },
            )?;
            conn.add_contact(
                "inbox_alice",
                ContactData {
                    display_name: Some("Alice".to_string()),
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
            )?;
            conn.add_contact(
                "inbox_bob",
                ContactData {
                    display_name: Some("Bob".to_string()),
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
            )?;

            // Sort by display name ascending
            let results = conn.get_contacts(Some(ContactsQuery {
                sort_by: Some(ContactSortField::DisplayName),
                sort_direction: Some(SortDirection::Ascending),
                ..Default::default()
            }))?;
            assert_eq!(results[0].display_name, Some("Alice".to_string()));
            assert_eq!(results[1].display_name, Some("Bob".to_string()));
            assert_eq!(results[2].display_name, Some("Charlie".to_string()));

            // Sort by display name descending
            let results = conn.get_contacts(Some(ContactsQuery {
                sort_by: Some(ContactSortField::DisplayName),
                sort_direction: Some(SortDirection::Descending),
                ..Default::default()
            }))?;
            assert_eq!(results[0].display_name, Some("Charlie".to_string()));
            assert_eq!(results[1].display_name, Some("Bob".to_string()));
            assert_eq!(results[2].display_name, Some("Alice".to_string()));

            // Sort by inbox_id ascending
            let results = conn.get_contacts(Some(ContactsQuery {
                sort_by: Some(ContactSortField::InboxId),
                sort_direction: Some(SortDirection::Ascending),
                ..Default::default()
            }))?;
            assert_eq!(results[0].inbox_id, "inbox_alice");
            assert_eq!(results[1].inbox_id, "inbox_bob");
            assert_eq!(results[2].inbox_id, "inbox_charlie");
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_combined_query() {
        with_connection(|conn| {
            // Add contacts with emails
            conn.add_contact(
                "inbox_1",
                ContactData {
                    display_name: Some("Alice Tester".to_string()),
                    is_favorite: Some(true),
                    ..Default::default()
                },
            )?;
            conn.add_email("inbox_1", "alice@test.com".to_string(), None)?;

            conn.add_contact(
                "inbox_2",
                ContactData {
                    display_name: Some("Bob Tester".to_string()),
                    is_favorite: Some(true),
                    ..Default::default()
                },
            )?;
            conn.add_email("inbox_2", "bob@test.com".to_string(), None)?;

            conn.add_contact(
                "inbox_3",
                ContactData {
                    display_name: Some("Charlie Tester".to_string()),
                    is_favorite: Some(false),
                    ..Default::default()
                },
            )?;
            conn.add_email("inbox_3", "charlie@test.com".to_string(), None)?;

            // Search for "tester", filter by favorite, sort by display name desc, limit 1
            let results = conn.get_contacts(Some(ContactsQuery {
                search: Some("tester".to_string()),
                is_favorite: Some(true),
                sort_by: Some(ContactSortField::DisplayName),
                sort_direction: Some(SortDirection::Descending),
                limit: Some(1),
                ..Default::default()
            }))?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].display_name, Some("Bob Tester".to_string()));
            assert!(results[0].is_favorite);
        })
    }
}
