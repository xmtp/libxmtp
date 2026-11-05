use super::{History, PendingEnvelope, Projection, Store};
use crate::error::Error;
use sqlx::{Postgres, Transaction};

impl Store {
    /// Read the complete identity-topic history from the primary.
    ///
    /// The returned head is the sequence ID of the final payload, or zero for
    /// an empty topic. Callers validate against this payload list and later
    /// compare the same head while holding the identity lock. Each payload's
    /// `server_ns` is returned beside it for the interval between claims.
    #[xmtp_common::db_span]
    pub(crate) async fn history(&self, topic: &[u8]) -> Result<History, Error> {
        let rows = sqlx::query!(
            "SELECT sequence_id, server_ns, payload FROM envelopes WHERE topic = $1 ORDER BY sequence_id",
            topic
        )
        .fetch_all(&self.primary)
        .await?;
        let head = rows.last().map_or(0, |row| row.sequence_id);
        let (payloads, server_ns) = rows
            .into_iter()
            .map(|row| (row.payload, row.server_ns))
            .unzip();
        Ok(History {
            head,
            payloads,
            server_ns,
        })
    }
}

/// Apply the identity projection in the same transaction as its envelope.
///
/// Added identifiers become active at `id`; removed identifiers retain their
/// history and receive a revocation sequence. The sequence predicates prevent
/// an older update from overwriting a newer association or revocation.
#[xmtp_common::db_span]
pub(crate) async fn apply_projection(
    tx: &mut Transaction<'_, Postgres>,
    inbox: &[u8],
    id: i64,
    changes: &Projection,
) -> Result<(), Error> {
    let (identifiers, kinds): (Vec<_>, Vec<_>) = changes.added.iter().cloned().unzip();
    sqlx::query!(
        "INSERT INTO identifier_association AS current
            (identifier, identifier_kind, inbox_id, association_sequence_id, revocation_sequence_id)
        SELECT r.identifier, r.kind, $3::bytea, $4::bigint, NULL
        FROM unnest($1::text[], $2::smallint[]) AS r(identifier, kind)
        ON CONFLICT (identifier, identifier_kind, inbox_id) DO UPDATE
        SET association_sequence_id = EXCLUDED.association_sequence_id, revocation_sequence_id = NULL
        WHERE EXCLUDED.association_sequence_id > current.association_sequence_id
            AND (current.revocation_sequence_id IS NULL
                OR EXCLUDED.association_sequence_id > current.revocation_sequence_id)",
        &identifiers, &kinds, inbox, id
    ).execute(&mut **tx).await?;
    let (identifiers, kinds): (Vec<_>, Vec<_>) = changes.removed.iter().cloned().unzip();
    sqlx::query!(
        "UPDATE identifier_association AS current SET revocation_sequence_id = $4::bigint
        FROM unnest($1::text[], $2::smallint[]) AS r(identifier, kind)
        WHERE current.identifier = r.identifier AND current.identifier_kind = r.kind
            AND current.inbox_id = $3::bytea AND current.association_sequence_id < $4::bigint
            AND (current.revocation_sequence_id IS NULL OR current.revocation_sequence_id < $4::bigint)",
        &identifiers, &kinds, inbox, id
    ).execute(&mut **tx).await?;
    Ok(())
}

/// Record the inbox that added each installation the new updates add, in the
/// same transaction as their envelopes.
///
/// The caller has already refused an installation another inbox added, so a
/// row that exists names the same inbox and is kept. A conflict with another
/// inbox fails the primary key and aborts the transaction.
#[xmtp_common::db_span]
pub(crate) async fn record_installations(
    tx: &mut Transaction<'_, Postgres>,
    new: &[&PendingEnvelope],
) -> Result<(), Error> {
    let (keys, inboxes): (Vec<Vec<u8>>, Vec<Vec<u8>>) = new
        .iter()
        .filter_map(|item| item.identity.as_ref())
        .flat_map(|identity| {
            identity
                .installations
                .iter()
                .map(|key| (key.clone(), identity.inbox_id.clone()))
        })
        .unzip();
    if keys.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "INSERT INTO installation_association (installation_key, inbox_id)
        SELECT DISTINCT r.installation_key, r.inbox_id
        FROM unnest($1::bytea[], $2::bytea[]) AS r(installation_key, inbox_id)
        WHERE NOT EXISTS (
            SELECT 1 FROM installation_association a
            WHERE a.installation_key = r.installation_key AND a.inbox_id = r.inbox_id
        )",
        &keys,
        &inboxes
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
