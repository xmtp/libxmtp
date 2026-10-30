use super::{PendingEnvelope, Store, StoredMeta, identity::apply_projection};
use crate::error::{AdmissionError, Error};
use crate::handles::HandleRejection;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use std::collections::HashSet;

#[cfg(test)]
mod tests;

use super::{ALLOCATION_BARRIER, GLOBAL_LOCK_DOMAIN, IDENTITY_LOCK};

impl Store {
    /// Mark envelopes already present on the primary as duplicates.
    ///
    /// This pass runs before validation. `commit_publish` repeats it after
    /// acquiring locks, because a copy may commit while validation is running.
    /// The stored metadata remains attached to every original request position.
    #[xmtp_common::db_span]
    pub(crate) async fn find_duplicates(
        &self,
        pending: &mut [PendingEnvelope],
    ) -> Result<(), Error> {
        if pending.is_empty() {
            return Ok(());
        }
        duplicates(&mut *self.primary.acquire().await?, pending).await
    }

    /// Atomically finish a publish after parsing and validation.
    ///
    /// The transaction locks identity state and topics, repeats duplicate
    /// lookup, checks each identity history head and each claimed handle,
    /// inserts new rows, advances watermarks, and applies projections before
    /// commit. A duplicate found by
    /// the locked check succeeds even when an earlier validation for that copy
    /// failed. A non-duplicate validation error or stale head aborts the whole
    /// transaction. The transaction timeout bounds database work; dropping the
    /// transaction rolls it back and releases its locks.
    #[xmtp_common::db_span]
    // implements: API-220, API-222
    pub(crate) async fn commit_publish(
        &self,
        pending: &mut [PendingEnvelope],
        max_duration_ms: u64,
    ) -> Result<Vec<StoredMeta>, Error> {
        if pending.iter().all(|item| item.duplicate.is_some()) {
            return pending
                .iter()
                .map(|item| {
                    item.duplicate
                        .clone()
                        .ok_or(Error::Invariant("publish metadata missing"))
                })
                .collect();
        }
        let mut tx = locks(&self.primary, pending, max_duration_ms).await?;
        // A committed copy overrides an earlier validation failure. Retain its
        // original metadata before checking admission for the remaining inputs.
        duplicates(&mut tx, pending).await?;
        check_heads(&mut tx, pending).await?;
        check_handles(&mut tx, pending).await?;
        let validation_error = pending
            .iter()
            .filter(|item| item.duplicate.is_none())
            .find_map(|item| {
                item.validation
                    .as_ref()
                    .err()
                    .map(|error| (item.index, error.clone()))
            });
        if let Some((index, error)) = validation_error {
            return Err(Error::Admission { index, error });
        }
        let new: Vec<_> = pending
            .iter()
            .filter(|item| item.duplicate.is_none())
            .collect();
        let inserted = insert(&mut tx, &new).await?;
        for (item, meta) in new.iter().zip(&inserted) {
            if let Ok(Some(changes)) = &item.validation {
                apply_projection(
                    &mut tx,
                    &item
                        .identity
                        .as_ref()
                        .ok_or(Error::Invariant("projection without identity"))?
                        .inbox_id,
                    meta.sequence_id,
                    changes,
                )
                .await?;
            }
        }
        tx.commit().await.map_err(Error::from)?;
        let mut inserted = inserted.into_iter();
        pending
            .iter()
            .map(|item| {
                item.duplicate
                    .clone()
                    .or_else(|| inserted.next())
                    .ok_or(Error::Invariant("publish metadata missing"))
            })
            .collect()
    }
}

/// Measure BEGIN and lock acquisition before the duplicate and validation rechecks.
#[xmtp_common::span(prefix = "publish")]
async fn locks(
    pool: &PgPool,
    pending: &[PendingEnvelope],
    max_duration_ms: u64,
) -> Result<Transaction<'static, Postgres>, Error> {
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
        .await?;
    let transaction_timeout = format!("{max_duration_ms}ms");
    sqlx::query!(
        "SELECT set_config('transaction_timeout', $1, true)",
        transaction_timeout
    )
    .fetch_one(&mut *tx)
    .await?;
    lock(&mut tx, pending).await?;
    Ok(tx)
}

/// Find stored rows by the request's `(topic, message_hash)` keys.
///
/// The ordinality join maps each database match back to its pending input. It
/// does not decide whether an absent row is valid; the caller performs that
/// validation and may run this lookup again under the publish locks.
async fn duplicates(
    connection: &mut PgConnection,
    pending: &mut [PendingEnvelope],
) -> Result<(), Error> {
    if pending.is_empty() {
        return Ok(());
    }
    let topics: Vec<_> = pending.iter().map(|item| item.topic.clone()).collect();
    let hashes: Vec<_> = pending
        .iter()
        .map(|item| item.message_hash.to_vec())
        .collect();
    let rows = sqlx::query!(
        r#"SELECT wanted.ordinality AS "ordinality!", e.sequence_id, e.topic, e.server_ns,
            e.expiry_ns, e.message_hash, e.is_commit_or_proposal
        FROM unnest($1::bytea[], $2::bytea[]) WITH ORDINALITY AS wanted(topic, hash, ordinality)
        JOIN envelopes e ON e.topic = wanted.topic AND e.message_hash = wanted.hash
        ORDER BY wanted.ordinality"#,
        &topics,
        &hashes
    )
    .fetch_all(connection)
    .await?;
    for row in rows {
        let index = usize::try_from(row.ordinality - 1)
            .map_err(|_| Error::Invariant("invalid duplicate ordinal"))?;
        let item = pending
            .get_mut(index)
            .ok_or(Error::Invariant("unknown duplicate ordinal"))?;
        item.duplicate = Some(StoredMeta {
            sequence_id: row.sequence_id,
            topic: row.topic,
            server_ns: row.server_ns,
            expiry_ns: row.expiry_ns,
            message_hash: row.message_hash,
            is_commit_or_proposal: row.is_commit_or_proposal,
        });
    }
    Ok(())
}

/// Acquire publish locks in one fixed order.
///
/// Identity updates share a global lock. Topic locks are derived from topic
/// hashes, sorted, and deduplicated to avoid lock-order deadlocks. The shared
/// allocation barrier excludes boundary maintenance while this transaction
/// allocates sequence IDs.
async fn lock(
    tx: &mut Transaction<'_, Postgres>,
    pending: &[PendingEnvelope],
) -> Result<(), Error> {
    if pending.iter().any(|item| item.identity.is_some()) {
        sqlx::query!(
            "SELECT pg_advisory_xact_lock($1::integer, $2::integer)",
            GLOBAL_LOCK_DOMAIN,
            IDENTITY_LOCK
        )
        .execute(&mut **tx)
        .await?;
    }
    let mut keys: Vec<_> = pending
        .iter()
        .map(|item| {
            let hash = xmtp_common::sha256_array(&item.topic);
            let mut key = [0; 8];
            key.copy_from_slice(&hash[..8]);
            i64::from_be_bytes(key)
        })
        .collect();
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        sqlx::query!("SELECT pg_advisory_xact_lock($1::bigint)", key)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query!(
        "SELECT pg_advisory_xact_lock_shared($1::integer, $2::integer)",
        GLOBAL_LOCK_DOMAIN,
        ALLOCATION_BARRIER
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Confirm that each identity update still follows the history it validated.
///
/// This check runs under the identity lock and compares the stored watermark
/// with the exact head captured during validation. A mismatch returns a stale
/// history error instead of silently validating against a newer state. An
/// update that failed before its history was read has no head and is not
/// compared, so its own rejection is reported and never becomes a stale
/// history.
// implements: API-303
async fn check_heads(
    tx: &mut Transaction<'_, Postgres>,
    pending: &[PendingEnvelope],
) -> Result<(), Error> {
    let (topics, heads): (Vec<_>, Vec<_>) = pending
        .iter()
        .filter(|item| item.duplicate.is_none())
        .filter_map(|item| Some((item.topic.clone(), item.identity.as_ref()?.head?)))
        .unzip();
    if topics.is_empty() {
        return Ok(());
    }
    let stale = sqlx::query!(
        r#"SELECT EXISTS (
            SELECT 1 FROM unnest($1::bytea[], $2::bigint[]) AS wanted(topic, head)
            LEFT JOIN topic_watermark w ON w.topic = wanted.topic
            WHERE COALESCE(w.last_sequence_id, 0) <> wanted.head
        ) AS "stale!""#,
        &topics,
        &heads
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(Error::from)?
    .stale;
    if stale {
        Err(Error::StaleHistory)
    } else {
        Ok(())
    }
}

/// Fail each new claim whose handle another inbox holds, or that a lower
/// index of this request claims for another inbox.
///
/// This runs under the identity lock, after the duplicate recheck, so a claim
/// that commits in a concurrent request is visible here and two inboxes never
/// both store one handle. Only claims that passed every earlier check are
/// marked, so a claim's earlier failure keeps its own reason. The unique index
/// on handles stays a backstop that fails as an invariant.
// implements: API-296
async fn check_handles(
    tx: &mut Transaction<'_, Postgres>,
    pending: &mut [PendingEnvelope],
) -> Result<(), Error> {
    let mut claims: Vec<_> = pending
        .iter()
        .enumerate()
        .filter(|(_, item)| item.duplicate.is_none() && item.validation.is_ok())
        .filter_map(|(position, item)| {
            let identity = item.identity.as_ref()?;
            let handle = identity.claim.as_ref()?;
            Some((item.index, position, handle.as_str(), &identity.inbox_id))
        })
        .collect();
    if claims.is_empty() {
        return Ok(());
    }
    claims.sort_unstable_by_key(|(index, ..)| *index);
    let mut taken = HashSet::new();
    // Two distinct updates for one inbox never share a request, so a repeat
    // here is a later claim by another inbox.
    let mut claimed = HashSet::new();
    for (_, position, handle, _) in &claims {
        if !claimed.insert(*handle) {
            taken.insert(*position);
        }
    }
    let handles: Vec<String> = claims
        .iter()
        .map(|(_, _, handle, _)| handle.to_string())
        .collect();
    let inboxes: Vec<Vec<u8>> = claims.iter().map(|(.., inbox)| inbox.to_vec()).collect();
    // The kind is a literal so the planner can use the partial unique index.
    let held = sqlx::query!(
        r#"SELECT wanted.ordinality AS "ordinality!"
        FROM unnest($1::text[], $2::bytea[]) WITH ORDINALITY AS wanted(handle, inbox_id, ordinality)
        WHERE EXISTS (
            SELECT 1 FROM identifier_association a
            WHERE a.identifier = wanted.handle AND a.identifier_kind = 3
                AND a.inbox_id <> wanted.inbox_id
        )"#,
        &handles,
        &inboxes
    )
    .fetch_all(&mut **tx)
    .await?;
    for row in held {
        let ordinal = usize::try_from(row.ordinality - 1)
            .map_err(|_| Error::Invariant("invalid handle ordinal"))?;
        let (_, position, ..) = claims
            .get(ordinal)
            .ok_or(Error::Invariant("unknown handle ordinal"))?;
        taken.insert(*position);
    }
    for position in taken {
        pending[position].validation = Err(AdmissionError::Handle(HandleRejection::Taken));
    }
    Ok(())
}

/// Allocate sequence IDs and insert all new envelopes in request order.
///
/// The statement uses the transaction timestamp for new rows, computes expiry, and
/// advances every affected topic watermark in the same transaction. Duplicate
/// resolution is complete before this function; an unexpected unique conflict
/// is therefore an invariant failure, not a successful duplicate.
async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    new: &[&PendingEnvelope],
) -> Result<Vec<StoredMeta>, Error> {
    if new.is_empty() {
        return Ok(Vec::new());
    }
    let count =
        i64::try_from(new.len()).map_err(|_| Error::Invariant("publish input count overflow"))?;
    let ids = sqlx::query!(
        r#"SELECT nextval('envelope_sequence') AS "id!" FROM generate_series(1, $1::bigint) ordinal ORDER BY ordinal"#, count
    ).fetch_all(&mut **tx).await?.into_iter().map(|row| row.id).collect::<Vec<_>>();
    let topics: Vec<_> = new.iter().map(|item| item.topic.clone()).collect();
    let hashes: Vec<_> = new.iter().map(|item| item.message_hash.to_vec()).collect();
    let flags: Vec<_> = new.iter().map(|item| item.is_commit_or_proposal).collect();
    let payloads: Vec<_> = new.iter().map(|item| item.payload.clone()).collect();
    let retention: Vec<_> = new.iter().map(|item| item.retention_ns).collect();
    let push_eligible: Vec<_> = new.iter().map(|item| item.push_eligible).collect();
    let sender_hmac: Vec<_> = new.iter().map(|item| item.sender_hmac.clone()).collect();
    // Advance watermarks from RETURNING rows; sibling CTEs share one snapshot.
    let rows = sqlx::query!(
        r#"WITH input AS (
            SELECT r.*, (extract(epoch FROM CURRENT_TIMESTAMP) * 1000000000)::bigint AS server_ns
            FROM unnest($1::bigint[], $2::bytea[], $3::bytea[], $4::boolean[], $5::bytea[], $6::bigint[], $7::boolean[], $8::bytea[])
            WITH ORDINALITY AS r(sequence_id, topic, message_hash, is_commit_or_proposal, payload, retention_ns, push_eligible, sender_hmac, ordinal)
        ), inserted AS (
            INSERT INTO envelopes (sequence_id, topic, message_hash, is_commit_or_proposal, payload, server_ns, expiry_ns, push_eligible, sender_hmac)
            SELECT sequence_id, topic, message_hash, is_commit_or_proposal, payload, server_ns, server_ns + retention_ns, push_eligible, sender_hmac
            FROM input ORDER BY ordinal
            RETURNING sequence_id, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal
        ), new_heads AS (
            SELECT topic, max(sequence_id) AS sequence_id FROM inserted GROUP BY topic
        ), advanced AS (
            INSERT INTO topic_watermark AS current (topic, last_sequence_id)
            SELECT topic, sequence_id FROM new_heads
            ON CONFLICT (topic) DO UPDATE SET last_sequence_id = EXCLUDED.last_sequence_id
            WHERE current.last_sequence_id < EXCLUDED.last_sequence_id RETURNING topic
        )
        SELECT inserted.sequence_id AS "sequence_id!", inserted.topic AS "topic!",
            inserted.server_ns AS "server_ns!", inserted.expiry_ns,
            inserted.message_hash AS "message_hash!", inserted.is_commit_or_proposal AS "is_commit_or_proposal!",
            (SELECT count(*) FROM advanced) = (SELECT count(*) FROM new_heads) AS "advanced!"
        FROM input JOIN inserted USING (sequence_id) ORDER BY input.ordinal"#,
        &ids, &topics, &hashes, &flags, &payloads, &retention as &[Option<i64>],
        &push_eligible, &sender_hmac as &[Option<Vec<u8>>]
    ).fetch_all(&mut **tx).await?;
    if rows.len() != new.len() || rows.iter().any(|row| !row.advanced) {
        return Err(Error::Invariant(
            "watermark did not advance for each inserted topic",
        ));
    }
    Ok(rows
        .into_iter()
        .map(|row| StoredMeta {
            sequence_id: row.sequence_id,
            topic: row.topic,
            server_ns: row.server_ns,
            expiry_ns: row.expiry_ns,
            message_hash: row.message_hash,
            is_commit_or_proposal: row.is_commit_or_proposal,
        })
        .collect())
}
