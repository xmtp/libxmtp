//! Recipient writes serialize on the recipient row before changing subscriptions.

use subtle::ConstantTimeEq;

use super::{
    PushRecipientRecord, PushSubscriptionRecord, RecipientStateRecord, Store, SubscriptionChanges,
};
use crate::error::Error;

impl Store {
    /// Read ownership and delivery from the primary, without replica lag.
    #[xmtp_common::db_span]
    pub(crate) async fn load_recipient(
        &self,
        recipient_id: &[u8],
    ) -> Result<Option<PushRecipientRecord>, Error> {
        let row = sqlx::query!(
            "SELECT recipient_id, secret_hash, channel, delivery, signing_key, metadata, renewed_ns FROM push_recipient WHERE recipient_id = $1",
            recipient_id
        ).fetch_optional(&self.primary).await?;
        row.map(|row| {
            Ok(PushRecipientRecord {
                recipient_id: row.recipient_id,
                secret_hash: row.secret_hash,
                channel: row.channel.try_into()?,
                delivery: row.delivery,
                signing_key: row.signing_key,
                metadata: row.metadata,
                renewed_ns: row.renewed_ns,
            })
        })
        .transpose()
    }

    /// Replace delivery while preserving subscriptions. The conflict predicate
    /// also checks ownership when two first registrations race.
    #[xmtp_common::db_span]
    pub(crate) async fn upsert_recipient(
        &self,
        record: &PushRecipientRecord,
    ) -> Result<RecipientStateRecord, Error> {
        let row = sqlx::query!(
            "INSERT INTO push_recipient (recipient_id, secret_hash, channel, delivery, signing_key, metadata, topic_count, renewed_ns)
            VALUES ($1, $2, $3, $4, $5, $6, 0, $7)
            ON CONFLICT (recipient_id) DO UPDATE SET channel = EXCLUDED.channel,
                delivery = EXCLUDED.delivery, signing_key = EXCLUDED.signing_key,
                metadata = EXCLUDED.metadata, renewed_ns = EXCLUDED.renewed_ns
            WHERE push_recipient.secret_hash = $2
            RETURNING topic_count, channel, renewed_ns",
            &record.recipient_id, &record.secret_hash, record.channel as i16,
            &record.delivery, record.signing_key.as_deref(), &record.metadata, record.renewed_ns
        ).fetch_optional(&self.primary).await?.ok_or(Error::PushSecretInvalid)?;
        Ok(RecipientStateRecord {
            topic_count: row.topic_count,
            channel: row.channel.try_into()?,
            renewed_ns: row.renewed_ns,
        })
    }

    /// Delete the owned recipient and cascade its subscriptions in one statement.
    #[xmtp_common::db_span]
    pub(crate) async fn delete_recipient(
        &self,
        recipient_id: &[u8],
        secret_hash: &[u8],
    ) -> Result<bool, Error> {
        Ok(sqlx::query!(
            "DELETE FROM push_recipient WHERE recipient_id = $1 AND secret_hash = $2",
            recipient_id,
            secret_hash
        )
        .execute(&self.primary)
        .await?
        .rows_affected()
            != 0)
    }

    /// Apply one atomic change set under the recipient lock. New subscriptions
    /// start at the closed boundary; replacements keep their start position.
    /// Cancellation or a limit failure rolls back every change in the request.
    #[xmtp_common::db_span]
    pub(crate) async fn apply_subscriptions(
        &self,
        recipient_id: &[u8],
        secret_hash: &[u8],
        adds: &[PushSubscriptionRecord],
        removes: &[Vec<u8>],
        max_topics: i32,
        renewed_ns: i64,
    ) -> Result<SubscriptionChanges, Error> {
        let mut tx = self.primary.begin().await?;
        let recipient = sqlx::query!(
            "SELECT secret_hash, topic_count, channel FROM push_recipient WHERE recipient_id = $1 FOR UPDATE",
            recipient_id
        ).fetch_optional(&mut *tx).await?.ok_or(Error::PushRecipientMissing)?;
        if !bool::from(recipient.secret_hash.as_slice().ct_eq(secret_hash)) {
            return Err(Error::PushSecretInvalid);
        }
        let boundary = sqlx::query_scalar!(
            "SELECT closed_sequence_id FROM allocation_boundary WHERE singleton"
        )
        .fetch_one(&mut *tx)
        .await?;
        let removed = sqlx::query!(
            "DELETE FROM push_subscription WHERE recipient_id = $1 AND topic = ANY($2::bytea[])",
            recipient_id,
            removes
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let topics: Vec<_> = adds.iter().map(|item| item.topic.clone()).collect();
        let epochs: Vec<_> = adds.iter().map(|item| item.hmac_epoch_base).collect();
        let key_0: Vec<_> = adds.iter().map(|item| item.hmac_keys[0].clone()).collect();
        let key_1: Vec<_> = adds.iter().map(|item| item.hmac_keys[1].clone()).collect();
        let key_2: Vec<_> = adds.iter().map(|item| item.hmac_keys[2].clone()).collect();
        let flags: Vec<_> = adds.iter().map(|item| item.include_commits).collect();
        let added = sqlx::query_scalar!(
            r#"INSERT INTO push_subscription (recipient_id, topic, since_sequence_id, hmac_epoch_base, hmac_key_0, hmac_key_1, hmac_key_2, include_commits)
            SELECT $1, topic, $2, epoch, key_0, key_1, key_2, include_commits
            FROM unnest($3::bytea[], $4::bigint[], $5::bytea[], $6::bytea[], $7::bytea[], $8::boolean[])
                AS input(topic, epoch, key_0, key_1, key_2, include_commits)
            ON CONFLICT (recipient_id, topic) DO UPDATE SET
                hmac_epoch_base = EXCLUDED.hmac_epoch_base, hmac_key_0 = EXCLUDED.hmac_key_0,
                hmac_key_1 = EXCLUDED.hmac_key_1, hmac_key_2 = EXCLUDED.hmac_key_2,
                include_commits = EXCLUDED.include_commits
            RETURNING (xmax = 0) AS "inserted!""#,
            recipient_id, boundary, &topics, &epochs as &[Option<i64>],
            &key_0 as &[Option<Vec<u8>>], &key_1 as &[Option<Vec<u8>>],
            &key_2 as &[Option<Vec<u8>>], &flags
        ).fetch_all(&mut *tx).await?.into_iter().filter(|inserted| *inserted).count() as u64;
        let count = i64::from(recipient.topic_count) - removed as i64 + added as i64;
        if count > i64::from(max_topics) {
            return Err(Error::PushTopicLimit);
        }
        let topic_count =
            i32::try_from(count).map_err(|_| Error::Invariant("push topic count overflow"))?;
        sqlx::query!(
            "UPDATE push_recipient SET topic_count = $2, renewed_ns = $3 WHERE recipient_id = $1",
            recipient_id,
            topic_count,
            renewed_ns
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(SubscriptionChanges {
            state: RecipientStateRecord {
                topic_count,
                channel: recipient.channel.try_into()?,
                renewed_ns,
            },
            added,
            removed,
        })
    }
}
