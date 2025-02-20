use super::{
    db_connection::DbConnection,
    schema::consent_records::{self, dsl},
    Sqlite,
};
use crate::{impl_store, storage::StorageError};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    upsert::excluded,
};
use serde::{Deserialize, Serialize};
use xmtp_id::associations::{ident, RootIdentifier};

/// StoredConsentRecord holds a serialized ConsentRecord
#[derive(Insertable, Queryable, Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[diesel(table_name = consent_records)]
#[diesel(primary_key(entity_type, entity))]
pub struct StoredConsentRecord {
    /// Enum, [`ConsentType`] representing the type of consent (conversation_id inbox_id, etc..)
    pub entity_type: StoredConsentType,
    /// Enum, [`ConsentState`] representing the state of consent (allowed, denied, etc..)
    pub state: ConsentState,
    /// The entity of what was consented (0x00 etc..)
    pub entity: String,
    // If entity_type is set to "Identity", what kind of identity is it?
    pub identity_kind: Option<StoredIdentityKind>,
}

impl StoredConsentRecord {
    pub fn new(entity: ConsentEntity, state: ConsentState) -> Self {
        Self {
            state,
            entity: entity.id(),
            entity_type: entity.r#type(),
            identity_kind: entity.kind(),
        }
    }

    pub fn root_identifier(&self) -> Option<RootIdentifier> {
        let identity_kind = &self.identity_kind?;
        let entity = &self.entity;
        let ident = match identity_kind {
            StoredIdentityKind::Ethereum => {
                RootIdentifier::Ethereum(ident::Ethereum(entity.clone()))
            }
            StoredIdentityKind::Passkey => {
                RootIdentifier::Passkey(ident::Passkey(hex::decode(entity).ok()?))
            }
        };
        Some(ident)
    }

    pub fn entity(&self) -> Result<ConsentEntity, StorageError> {
        let entity = match self.entity_type {
            StoredConsentType::ConversationId => {
                ConsentEntity::ConversationId(self.entity.to_string())
            }
            StoredConsentType::Identity => ConsentEntity::Identity(
                self.root_identifier()
                    .expect("Field is required in database when type is set to identity"),
            ),
            StoredConsentType::InboxId => ConsentEntity::InboxId(hex::decode(&self.entity)?),
        };
        Ok(entity)
    }
}

impl_store!(StoredConsentRecord, consent_records);

impl DbConnection {
    /// Returns the consent_records for the given entity up
    pub fn get_consent_record(
        &self,
        entity: &ConsentEntity,
    ) -> Result<Option<StoredConsentRecord>, StorageError> {
        Ok(self.raw_query_read(|conn| -> diesel::QueryResult<_> {
            dsl::consent_records
                .filter(dsl::entity.eq(entity.id()))
                .filter(dsl::entity_type.eq(entity.r#type()))
                .filter(dsl::identity_kind.eq(entity.kind()))
                .first(conn)
                .optional()
        })?)
    }

    /// Insert consent_records, and replace existing entries, returns records that are new or changed
    pub fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, StorageError> {
        let mut query = consent_records::table
            .into_boxed()
            .filter(false.into_sql::<diesel::sql_types::Bool>());
        let primary_keys: Vec<_> = records
            .iter()
            .map(|r| (&r.entity, &r.entity_type))
            .collect();
        for (entity, entity_type) in primary_keys {
            query = query.or_filter(
                consent_records::entity_type
                    .eq(entity_type)
                    .and(consent_records::entity.eq(entity)),
            );
        }

        let changed = self.raw_query_write(|conn| -> diesel::QueryResult<_> {
            let existing: Vec<StoredConsentRecord> = query.load(conn)?;
            let changed: Vec<_> = records
                .iter()
                .filter(|r| !existing.contains(r))
                .cloned()
                .collect();

            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                for record in records.iter() {
                    diesel::insert_into(dsl::consent_records)
                        .values(record)
                        .on_conflict((dsl::entity_type, dsl::entity))
                        .do_update()
                        .set(dsl::state.eq(excluded(dsl::state)))
                        .execute(conn)?;
                }
                Ok(())
            })?;

            Ok(changed)
        })?;

        Ok(changed)
    }

    pub fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, StorageError> {
        self.raw_query_write(|conn| {
            let maybe_inserted_consent_record: Option<StoredConsentRecord> =
                diesel::insert_into(dsl::consent_records)
                    .values(record)
                    .on_conflict_do_nothing()
                    .get_result(conn)
                    .optional()?;

            // if record was not inserted...
            if maybe_inserted_consent_record.is_none() {
                return Ok(dsl::consent_records
                    .find((&record.entity_type, &record.entity))
                    .first(conn)
                    .optional()?);
            }

            Ok(None)
        })
    }
}

pub enum ConsentEntity {
    ConversationId(String),
    InboxId(Vec<u8>),
    Identity(RootIdentifier),
}

impl ConsentEntity {
    fn id(&self) -> String {
        match self {
            Self::ConversationId(id) => id.to_string(),
            Self::InboxId(id) => hex::encode(id),
            Self::Identity(ident) => format!("{ident}"),
        }
    }

    fn r#type(&self) -> StoredConsentType {
        match self {
            Self::ConversationId(_) => StoredConsentType::ConversationId,
            Self::InboxId(_) => StoredConsentType::InboxId,
            Self::Identity(_) => StoredConsentType::Identity,
        }
    }

    fn kind(&self) -> Option<StoredIdentityKind> {
        match self {
            Self::Identity(ident) => Some(ident.into()),
            _ => None,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// Type of consent record stored
pub enum StoredConsentType {
    /// Consent is for a conversation
    ConversationId = 1,
    /// Consent is for an inbox
    InboxId = 2,
    /// Consent is for an identity
    Identity = 3,
}

impl ToSql<Integer, Sqlite> for StoredConsentType
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for StoredConsentType
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(Self::ConversationId),
            2 => Ok(Self::InboxId),
            3 => Ok(Self::Identity),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// Type of identity stored
pub enum StoredIdentityKind {
    Ethereum = 1,
    Passkey = 2,
}

impl ToSql<Integer, Sqlite> for StoredIdentityKind
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for StoredIdentityKind
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(Self::Ethereum),
            2 => Ok(Self::Passkey),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl From<&RootIdentifier> for StoredIdentityKind {
    fn from(ident: &RootIdentifier) -> Self {
        match ident {
            RootIdentifier::Ethereum(_) => Self::Ethereum,
            RootIdentifier::Passkey(_) => Self::Passkey,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// The state of the consent
pub enum ConsentState {
    /// Consent is unknown
    Unknown = 0,
    /// Consent is allowed
    Allowed = 1,
    /// Consent is denied
    Denied = 2,
}

impl ToSql<Integer, Sqlite> for ConsentState
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for ConsentState
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            0 => Ok(ConsentState::Unknown),
            1 => Ok(ConsentState::Allowed),
            2 => Ok(ConsentState::Denied),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::encrypted_store::tests::with_connection;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn insert_and_read() {
        with_connection(|conn| {
            let inbox_id = b"inbox_1";
            let consent_record = StoredConsentRecord::new(
                ConsentEntity::InboxId(inbox_id.to_vec()),
                ConsentState::Allowed,
            );
            let consent_record_entity = consent_record.entity.clone();

            // Insert the record
            let result = conn
                .insert_or_replace_consent_records(&[consent_record.clone()])
                .expect("should store without error");
            // One record was inserted
            assert_eq!(result.len(), 1);

            // Insert it again
            let result = conn
                .insert_or_replace_consent_records(&[consent_record.clone()])
                .expect("should store without error");
            // Nothing should change
            assert_eq!(result.len(), 0);

            // Insert it again, this time with a Denied state
            let result = conn
                .insert_or_replace_consent_records(&[StoredConsentRecord {
                    state: ConsentState::Denied,
                    ..consent_record
                }])
                .expect("should store without error");
            // Should change
            assert_eq!(result.len(), 1);

            let consent_record = conn
                .get_consent_record(&ConsentEntity::InboxId(inbox_id.to_vec()))
                .expect("query should work");

            assert_eq!(consent_record.unwrap().entity, consent_record_entity);

            let conflict = StoredConsentRecord::new(
                ConsentEntity::InboxId(inbox_id.to_vec()),
                ConsentState::Allowed,
            );

            let existing = conn
                .maybe_insert_consent_record_return_existing(&conflict)
                .unwrap();
            assert!(existing.is_some());
            let existing = existing.unwrap();
            // we want the old record to be returned.
            assert_eq!(existing.state, ConsentState::Denied);

            let db_cr = conn
                .get_consent_record(&existing.entity().unwrap())
                .unwrap()
                .unwrap();
            // ensure the db matches the state of what was returned
            assert_eq!(db_cr.state, existing.state);
        })
        .await;
    }
}
