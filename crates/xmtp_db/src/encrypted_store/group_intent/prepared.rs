//! Exact prepared envelope bytes for an existing logical intent.

use diesel::prelude::*;

use crate::schema::group_intents as intents;
use crate::stream_storage::stream_transaction;
use crate::{ConnectionExt, NotFound, StorageError};

/// Exact attempt bytes kept across retries of one logical intent.
pub trait QueryPreparedEnvelope: ConnectionExt + Sized {
    /// Read the persisted attempt. None means no attempt is currently prepared.
    fn prepared_envelopes(&self, intent_id: super::ID) -> Result<Option<Vec<u8>>, StorageError> {
        self.raw_query(|conn| {
            intents::table
                .find(intent_id)
                .select(intents::prepared_envelopes)
                .first::<Option<Vec<u8>>>(conn)
                .optional()
        })?
        .ok_or_else(|| NotFound::IntentById(intent_id).into())
    }

    /// Replace only the exact attempt the caller read under the state writer.
    /// Late publish replies must use this check before attaching receipt metadata.
    fn compare_and_set_prepared_envelopes(
        &self,
        intent_id: super::ID,
        expected: Option<&[u8]>,
        replacement: Option<&[u8]>,
    ) -> Result<bool, StorageError> {
        stream_transaction(self, |conn| {
            let current = intents::table
                .find(intent_id)
                .select(intents::prepared_envelopes)
                .first::<Option<Vec<u8>>>(conn)
                .optional()?
                .ok_or(NotFound::IntentById(intent_id))?;
            if current.as_deref() != expected {
                return Ok(false);
            }
            diesel::update(intents::table.find(intent_id))
                .set(intents::prepared_envelopes.eq(replacement))
                .execute(conn)?;
            Ok(true)
        })
    }
}

impl<C: ConnectionExt> QueryPreparedEnvelope for C {}
