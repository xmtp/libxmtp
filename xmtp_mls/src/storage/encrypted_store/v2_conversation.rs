use crate::{impl_store, storage::StorageError};

use super::Sqlite;
use super::{
    db_connection::DbConnection,
    schema::consent_records::{self, dsl},
};
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

/// StoredV2Conversation holds a serialized ConsentRecord
#[derive(Insertable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = v2_conversations)]
#[diesel(primary_key(topic))]
pub struct StoredV2Conversation {
    pub topic: String,
    pub peer_address: String,
    pub envelope_bytes: Vec<u8>,
    pub created_at_ns: i64,
}

impl StoredV2Conversation {
    pub fn new(topic: String, peer_address: String, envelope_bytes: Vec<u8>, created_at_ns: i64) -> Self {
        Self {
            topic,
            peer_address,
            envelope_bytes,
            created_at_ns
        }
    }
}

impl_store!(StoredV2Conversation, v2_conversations);

impl DbConnection {
    /// Returns a v2_conversation
    pub fn get_v2_conversation(
        &self,
        peer_address: String,
    ) -> Result<Option<StoredV2Conversation>, StorageError> {
        Ok(self.raw_query(|conn| -> diesel::QueryResult<_> {
            dsl::v2_conversations
                .filter(dsl::peer_address.eq(peer_address))
                .first(conn)
                .optional()
        })?)
    }

    /// Returns all the v2_conversations
    pub fn get_v2_conversations(
        &self,
    ) -> Result<Vec<StoredV2Conversation>, StorageError> {
        Ok(self.raw_query(|conn| -> diesel::QueryResult<_> {
            dsl::v2_conversations
        })?)
    }

    /// Insert v2_conversations
    pub fn insert_or_replace_v2_conversation(&self, v2_conversation: StoredV2Conversation) -> Result<StoredV2Conversation, StorageError> {
        tracing::info!("Trying to insert v2 conversation");
        let stored_v2_conversation = self.raw_query(|conn| {
            let maybe_inserted_conversation: Option<StoredV2Conversation> = diesel::insert_into(dsl::v2_conversations)
                .values(&v2_conversation)
                .on_conflict_do_nothing()
                .get_result(conn)
                .optional()?;

            if maybe_inserted_conversation.is_none() {
                let existing_conversation: StoredV2Conversation = dsl::v2_conversations.find(v2_conversation.id).first(conn)?;
                if existing_conversation.welcome_id == v2_conversation.welcome_id {
                    tracing::info!("V2 Conversation invite already exists");
                    // Error so OpenMLS db transaction are rolled back on duplicate welcomes
                    return Err(StorageError::Duplicate(DuplicateItem::WelcomeId(
                        existing_group.welcome_id,
                    )));
                } else {
                    tracing::info!("V2 Conversation already exists");
                    return Ok(existing_group);
                }
            } else {
                tracing::info!("V2 Conversation is inserted");
            }

            match maybe_inserted_conversation {
                Some(v2_conversation) => Ok(v2_conversation),
                None => Ok(dsl::v2_conversations.find(v2_conversation.id).first(conn)?),
            }
        })?;

        Ok(stored_v2_conversation)
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::encrypted_store::tests::with_connection;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    // fn generate_consent_record(
    //     entity_type: ConsentType,
    //     state: ConsentState,
    //     entity: String,
    // ) -> StoredConsentRecord {
    //     StoredConsentRecord {
    //         entity_type,
    //         state,
    //         entity,
    //     }
    // }

    // #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    // #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    // async fn insert_and_read() {
    //     with_connection(|conn| {
    //         let inbox_id = "inbox_1";
    //         let consent_record = generate_consent_record(
    //             ConsentType::InboxId,
    //             ConsentState::Denied,
    //             inbox_id.to_string(),
    //         );
    //         let consent_record_entity = consent_record.entity.clone();

    //         conn.insert_or_replace_consent_records(vec![consent_record])
    //             .expect("should store without error");

    //         let consent_record = conn
    //             .get_consent_record(inbox_id.to_owned(), ConsentType::InboxId)
    //             .expect("query should work");

    //         assert_eq!(consent_record.unwrap().entity, consent_record_entity);
    //     })
    //     .await;
    // }
}
