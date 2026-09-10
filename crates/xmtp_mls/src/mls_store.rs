//! Higher level queries against the local database
//! These queries return their mls-typed equivalents after converting
//! from the data in DB/Api
use prost::Message;
use std::collections::HashMap;

use xmtp_api::ApiError;
use xmtp_common::RetryableError;
use xmtp_db::incoming_envelope::{
    AdmissionResult, IncomingLimits, NetworkEntityKind, NewIncomingEnvelope, StreamTopic,
};
use xmtp_db::{
    Fetch, NotFound, XmtpOpenMlsProvider,
    group::{GroupQueryArgs, StoredGroup},
};
use xmtp_proto::types::{
    GroupId, IncomingBatchLimits, InstallationId, OrderedEnvelopeBatch, Topic, TopicCursor,
    TopicKind,
};

use crate::{context::XmtpSharedContext, groups::MlsGroup};
use xmtp_id::key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2};

use thiserror::Error;
use xmtp_db::prelude::*;

#[derive(Error, Debug)]
pub enum MlsStoreError {
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Connection(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    NotFound(#[from] NotFound),
}

impl RetryableError for MlsStoreError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(e) => e.is_retryable(),
            Self::Api(e) => e.is_retryable(),
            Self::Connection(e) => e.is_retryable(),
            Self::NotFound(e) => e.is_retryable(),
        }
    }
}

impl crate::worker::NeedsDbReconnect for MlsStoreError {
    /// Forwards a dropped-pool signal from the storage/connection variants so a
    /// worker loading groups can stop on disconnect. `Api`/`NotFound` return `false`.
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Connection(c) => c.db_needs_connection(),
            Self::Api(_) | Self::NotFound(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct MlsStore<Context> {
    context: Context,
}

/// Durable admission results from one bounded unary receipt page.
#[derive(Debug)]
pub struct ReceivedPage {
    /// Each topic's committed receipt result; processing runs separately.
    pub admissions: Vec<(Topic, AdmissionResult)>,
    /// The backend reports another page after this response.
    pub has_more: bool,
}

fn stream_topic(topic: &Topic) -> Result<StreamTopic, ApiError> {
    let kind = match topic.kind() {
        TopicKind::GroupMessagesV1 => NetworkEntityKind::Group,
        TopicKind::WelcomeMessagesV1 => NetworkEntityKind::Welcome,
        TopicKind::IdentityUpdatesV1 => NetworkEntityKind::Identity,
        _ => return Err(ApiError::InvalidRequest("incoming topic kind")),
    };
    Ok(StreamTopic {
        entity_id: topic.identifier().to_vec(),
        kind,
    })
}

impl<Context> MlsStore<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> MlsStore<Context>
where
    Context: XmtpSharedContext,
{
    /// Read durable receipt F. Transport delivery alone does not advance it.
    pub fn received_cursors(&self, topics: &[Topic]) -> Result<TopicCursor, MlsStoreError> {
        let conn = self.context.db();
        topics
            .iter()
            .map(|topic| {
                Ok((
                    topic.clone(),
                    conn.topic_progress(&stream_topic(topic)?)?.received,
                ))
            })
            .collect()
    }

    /// Store a validated raw batch and its receipt position in one transaction.
    /// New Welcome rows queue key rotation in that same writer, before decoding.
    pub fn admit_incoming_batch(
        &self,
        batch: &OrderedEnvelopeBatch,
        limits: IncomingLimits,
    ) -> Result<AdmissionResult, MlsStoreError> {
        let topic = stream_topic(&batch.topic)?;
        let bytes = batch.envelopes.iter().try_fold(0u64, |bytes, envelope| {
            bytes.checked_add(envelope.encoded_len() as u64)
        });
        if batch.envelopes.len() as u64 > limits.batch.rows
            || bytes.is_none_or(|bytes| bytes > limits.batch.bytes)
        {
            return Err(
                ApiError::Envelope(xmtp_api_backend::envelope::EnvelopeError::Capacity).into(),
            );
        }
        let mut previous = batch.after;
        let mut rows = Vec::with_capacity(batch.envelopes.len());
        for envelope in &batch.envelopes {
            let meta = envelope
                .meta
                .as_ref()
                .ok_or(ApiError::InvalidResponse("incoming metadata"))?;
            let (envelope_topic, sequence_id, _) =
                xmtp_api_backend::envelope::metadata(meta, batch.topic.kind())
                    .map_err(ApiError::from)?;
            if envelope_topic != batch.topic || sequence_id <= previous {
                return Err(ApiError::InvalidResponse("incoming batch order").into());
            }
            previous = sequence_id;
            rows.push(NewIncomingEnvelope {
                sequence_id,
                envelope: envelope.encode_to_vec(),
            });
        }
        let admitted = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let admitted = storage
                .db()
                .admit_ordered_batch(&topic, batch.after, &rows, limits)?;
            if topic.kind == NetworkEntityKind::Welcome && admitted.inserted > 0 {
                crate::worker::key_package_maintenance::queue_key_rotation_in(&storage)?;
            }
            Ok::<_, xmtp_db::StorageError>(xmtp_db::TransactionOutcome::Continue(admitted))
        })?
        .into_continued();
        if topic.kind == NetworkEntityKind::Welcome && admitted.inserted > 0 {
            self.context.task_channels().wake();
        }
        Ok(admitted)
    }

    /// Query from durable receipt positions and commit one bounded page.
    pub async fn receive_topics_once(
        &self,
        topics: &[Topic],
        limits: IncomingLimits,
    ) -> Result<ReceivedPage, MlsStoreError> {
        let cursors = self.received_cursors(topics)?;
        let rows = limits
            .batch
            .rows
            .min(limits.topic.rows)
            .min(limits.kind.rows)
            .min(u64::from(self.context.stream_settings().max_fetched_rows));
        let bytes = limits
            .batch
            .bytes
            .min(limits.topic.bytes)
            .min(limits.kind.bytes)
            .min(self.context.stream_settings().max_fetched_bytes);
        let page = self
            .context
            .api()
            .query_ordered_page(
                cursors,
                rows.min(xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_LIMIT as u64) as u32,
                IncomingBatchLimits {
                    max_rows: usize::try_from(rows).unwrap_or(usize::MAX),
                    max_bytes: usize::try_from(bytes).unwrap_or(usize::MAX),
                },
            )
            .await?;
        let mut admissions = Vec::with_capacity(page.batches.len());
        for batch in page.batches {
            let admitted = self.admit_incoming_batch(&batch, limits)?;
            admissions.push((batch.topic, admitted));
        }
        Ok(ReceivedPage {
            admissions,
            has_more: page.has_more,
        })
    }

    /// Fetches the current key package from the network for each of the `installation_id`s specified
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        MlsStoreError,
    > {
        let installation_ids = installation_ids
            .into_iter()
            .map(InstallationId::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::from)?;
        let key_package_results = self
            .context
            .api()
            .fetch_key_packages(&installation_ids)
            .await?;

        let crypto_provider = XmtpOpenMlsProvider::<()>::new_crypto();

        let results: HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>> =
            key_package_results
                .iter()
                .filter_map(|(id, package)| {
                    let package = package.as_ref()?;
                    Some((
                        id.to_vec(),
                        VerifiedKeyPackageV2::from_bytes(
                            &crypto_provider,
                            &package.key_package_tls_serialized,
                        ),
                    ))
                })
                .collect();

        Ok(results)
    }

    /// Query for groups with optional filters
    ///
    /// Filters:
    /// - allowed_states: only return groups with the given membership states
    /// - created_after_ns: only return groups created after the given timestamp (in nanoseconds)
    /// - created_before_ns: only return groups created before the given timestamp (in nanoseconds)
    /// - limit: only return the first `limit` groups
    pub fn find_groups(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<MlsGroup<Context>>, MlsStoreError> {
        Ok(self
            .context
            .db()
            .find_groups(args)?
            .into_iter()
            .map(|stored_group| {
                MlsGroup::new(
                    self.context.clone(),
                    stored_group.id,
                    stored_group.dm_id,
                    stored_group.conversation_type,
                    stored_group.created_at_ns,
                )
            })
            .collect())
    }

    /// Look up a group by its ID
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn group(&self, group_id: &GroupId) -> Result<MlsGroup<Context>, MlsStoreError> {
        let conn = self.context.db();
        let stored_group: Option<StoredGroup> = conn.fetch(group_id)?;
        stored_group
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .ok_or(NotFound::GroupById(*group_id))
            .map_err(Into::into)
    }
}
