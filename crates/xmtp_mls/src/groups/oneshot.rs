use super::{GroupError, MlsGroup, PreconfiguredPolicies};
use crate::context::XmtpSharedContext;
use xmtp_common::snippet::Snippet;
use xmtp_db::{
    MlsProviderExt, XmtpMlsStorageProvider,
    consent_record::{ConsentState, conversation_consent},
    group::ConversationType,
    prelude::QueryReaddStatus,
};
use xmtp_id::AsIdRef;
use xmtp_mls_common::{group::GroupMetadataOptions, group_metadata::GroupMetadata};
use xmtp_proto::{
    types::{Cursor, GroupId},
    xmtp::mls::message_contents::{OneshotMessage, oneshot_message},
};

pub struct Oneshot {}

impl Oneshot {
    /// Creates a oneshot group with the given message and adds the specified inbox IDs to it.
    ///
    /// A oneshot group is a special type of group that contains a single message and is used
    /// for specific messaging scenarios like readd requests where no long-lived group is needed.
    ///
    /// # Arguments
    /// * `context` - The shared context for group operations
    /// * `inbox_ids` - List of inbox IDs to add to the group. Note that the sender's other
    ///   installations are implicitly included and will also receive the oneshot message.
    /// * `oneshot_message` - The oneshot message to include in the group metadata
    ///
    /// # Returns
    /// An error if sending failed, otherwise nothing
    pub async fn send_message<C: XmtpSharedContext, S: AsIdRef>(
        context: C,
        inbox_ids: impl AsRef<[S]>,
        oneshot_message: OneshotMessage,
    ) -> Result<(), GroupError> {
        // Create a oneshot group with the oneshot message
        let group = MlsGroup::<C>::create_and_insert(
            context.clone(),
            ConversationType::Oneshot,
            PreconfiguredPolicies::default().to_policy_set(),
            GroupMetadataOptions::default(),
            Some(oneshot_message),
        )?;

        // Add the specified inbox IDs to the group
        if !inbox_ids.as_ref().is_empty() {
            group.add_members(&inbox_ids).await?;
        } else {
            group.add_missing_installations().await?;
        }

        // Optional: delete group from DB here
        Ok(())
    }

    pub fn process_message(
        provider: &impl MlsProviderExt,
        own_inbox_id: &str,
        sender_installation_id: Vec<u8>,
        message: OneshotMessage,
    ) -> Result<(), GroupError> {
        match message.message_type {
            Some(oneshot_message::MessageType::ReaddRequest(readd_request)) => {
                if Self::validate_readd_request(
                    provider.key_store(),
                    own_inbox_id,
                    &readd_request.group_id,
                    &sender_installation_id,
                )? {
                    let group_id = GroupId::try_from(readd_request.group_id.as_slice())?;
                    provider.key_store().db().update_requested_at_sequence_id(
                        &group_id,
                        &sender_installation_id,
                        readd_request.latest_commit_sequence_id as i64,
                    )?;
                    tracing::info!(
                        group_id = readd_request.group_id.snippet(),
                        sender_installation_id = sender_installation_id.snippet(),
                        latest_commit_sequence_id = readd_request.latest_commit_sequence_id,
                        "Stored incoming readd request"
                    );
                }
            }
            _ => {
                tracing::warn!(
                    "Oneshot message {:?} is not a recognized message type",
                    message.message_type
                );
            }
        }
        Ok(())
    }

    pub fn process_welcome(
        provider: &impl MlsProviderExt,
        own_inbox_id: &str,
        cursor: Cursor,
        sender_installation_id: Vec<u8>,
        metadata: GroupMetadata,
    ) -> Result<(), GroupError> {
        tracing::debug!("Processing oneshot welcome");
        if let Some(message) = metadata.oneshot_message {
            Self::process_message(provider, own_inbox_id, sender_installation_id, message)?;
        } else {
            tracing::warn!(
                "Oneshot group welcome {} does not have oneshot message",
                cursor
            );
        }
        Ok(())
    }

    // implements: FORK-053
    fn validate_readd_request(
        storage: &impl XmtpMlsStorageProvider,
        own_inbox_id: &str,
        group_id: &Vec<u8>,
        sender_installation_id: &Vec<u8>,
    ) -> Result<bool, GroupError> {
        let id = GroupId::try_from(group_id.as_slice())?;
        if conversation_consent(&storage.db(), own_inbox_id, &id)? != ConsentState::Allowed {
            tracing::warn!(
                group_id = group_id.snippet(),
                "Group not consented for readd request"
            );
            return Ok(false);
        }
        // Fetch the OpenMLS group without locking; consistency is not important here, and we are not writing to it
        let Ok(Some(openmls_group)) = openmls::group::MlsGroup::load(
            storage,
            &openmls::group::GroupId::from_slice(group_id.as_slice()),
        ) else {
            tracing::warn!(
                group_id = group_id.snippet(),
                "Unable to load OpenMLS group for readd request"
            );
            return Ok(false);
        };
        if !openmls_group
            .members()
            .any(|member| member.signature_key == *sender_installation_id)
        {
            tracing::warn!(
                group_id = group_id.snippet(),
                "Sender not a member of group for readd request"
            );
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(dead_code)]
#[path = "mls_sync/publish/tests/faults.rs"]
mod send_faults;
#[cfg(all(test, not(target_arch = "wasm32")))]
use xmtp_proto::backend_v1::client_envelope::Payload;

#[cfg(test)]
pub(crate) fn send_state(context: &impl XmtpSharedContext) -> Vec<(String, i64)> {
    use xmtp_db::{
        ConnectionExt,
        diesel::{prelude::*, sql_types::BigInt},
    };
    #[derive(QueryableByName)]
    struct Count {
        #[diesel(sql_type = BigInt)]
        n: i64,
    }
    context
        .db()
        .raw_query(|conn| {
            [
                "groups",
                "group_intents",
                "group_messages",
                "local_commit_log",
                "remote_commit_log",
                "refresh_state",
                "incoming_envelopes",
                "openmls_key_value",
            ]
            .into_iter()
            .map(|table| {
                diesel::sql_query(format!("SELECT COUNT(*) AS n FROM {table}"))
                    .get_result::<Count>(conn)
                    .map(|count| (table.to_string(), count.n))
            })
            .collect()
        })
        .unwrap()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::tester;
    use futures::stream::StreamExt;
    use xmtp_proto::xmtp::mls::message_contents::{ReaddRequest, oneshot_message::MessageType};

    #[rstest::rstest]
    #[case::refused(false)]
    #[case::published_but_unconfirmed(true)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn oneshot_readd_send_outcomes(#[case] published: bool) {
        use super::send_faults::{Fault, FaultyApi, faulty_group, group_message, welcome};
        use crate::groups::commit_log::CommitLogWorker;
        use xmtp_common::{
            RetryableError,
            time::{Duration, sleep},
        };
        use xmtp_db::prelude::{QueryGroup, QueryGroupIntent, QueryPreparedEnvelope};
        tester!(alix, disable_workers, enable_fork_recovery_requests);
        tester!(bo, disable_workers);
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        CommitLogWorker::new(alix.context.clone())._tick().await?;
        let before = send_state(&alix.context);
        let delay = Duration::from_millis(10);
        let api = FaultyApi::new(
            &alix,
            if published { welcome } else { group_message },
            [Fault::RateLimit(delay); 18],
        );
        let faulty = faulty_group(&alix, &group.group_id, api.clone()).await;
        let db = faulty.context.db();
        db.set_group_commit_log_forked_status(&group.group_id, Some(true))?;
        let mut worker = CommitLogWorker::new(faulty.context.clone());
        let error = worker._tick().await.unwrap_err();
        assert_eq!(error.rate_limit_delay(), Some(delay));
        assert_eq!(
            db.is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice())?,
            published,
            "only an unknown outcome keeps the latch"
        );
        if !published {
            assert_eq!(
                send_state(&alix.context),
                before,
                "a refused send leaves no temporary state"
            );
            let sent = api.requests.lock().len();
            sleep(delay).await;
            worker._tick().await?;
            assert!(
                api.requests.lock().len() > sent,
                "a refused request must retry after its delay"
            );
        } else {
            use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::groups};
            let id = db.raw_query(|conn| {
                groups::table
                    .filter(groups::conversation_type.eq(ConversationType::Oneshot))
                    .select(groups::id)
                    .first::<GroupId>(conn)
            })?;
            let intent = db.find_group_intents(id, None, None)?.pop().unwrap();
            let bytes = db.prepared_envelopes(intent.id)?.unwrap();
            let sent = api.requests.lock().len();
            worker._tick().await?;
            assert_eq!(
                api.requests.lock().len(),
                sent,
                "an unknown outcome must not send another request"
            );
            assert_eq!(
                db.prepared_envelopes(intent.id)?.unwrap(),
                bytes,
                "unknown-outcome bytes must survive"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_receive_oneshot_message_via_syncing() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(caro, disable_workers);
        let a_group = alix
            .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
            .await?;
        let group_id = a_group.group_id;
        for recipient in [&bo, &caro] {
            recipient.sync_all_welcomes_and_groups(None).await?;
            recipient
                .group(&group_id)?
                .update_consent_state(ConsentState::Allowed)?;
            assert!(
                recipient
                    .context
                    .db()
                    .get_readd_status(&group_id, alix.context.installation_id().as_slice())?
                    .is_none()
            );
        }
        let before = send_state(&alix.context);
        Oneshot::send_message(
            alix.context.clone(),
            [bo.inbox_id(), caro.inbox_id()],
            OneshotMessage {
                message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                    group_id: group_id.to_vec(),
                    latest_commit_sequence_id: 42,
                })),
            },
        )
        .await?;
        assert_eq!(
            send_state(&alix.context),
            before,
            "temporary sender state must be removed"
        );
        for recipient in [&bo, &caro] {
            recipient.sync_welcomes().await?;
            let status = recipient
                .context
                .db()
                .get_readd_status(&group_id, alix.context.installation_id().as_slice())?
                .unwrap();
            assert_eq!(status.requested_at_sequence_id, Some(42));
            assert_eq!(status.responded_at_sequence_id, None);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn oneshot_without_explicit_recipients_cleans_up() {
        tester!(alix, disable_workers);
        tester!(phone, from: alix, disable_workers);
        let group = alix.create_group(None, None)?;
        group.add_missing_installations().await?;
        phone.sync_all_welcomes_and_groups(None).await?;
        let before = send_state(&alix.context);
        Oneshot::send_message(
            alix.context.clone(),
            Vec::<String>::new(),
            OneshotMessage {
                message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                    group_id: group.group_id.to_vec(),
                    latest_commit_sequence_id: 42,
                })),
            },
        )
        .await?;
        assert_eq!(
            send_state(&alix.context),
            before,
            "empty-recipient sender state must be removed"
        );
        phone.sync_welcomes().await?;
        assert_eq!(
            phone
                .context
                .db()
                .get_readd_status(&group.group_id, alix.context.installation_id().as_slice())?
                .unwrap()
                .requested_at_sequence_id,
            Some(42)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_oneshot_groups_not_in_find_groups() {
        tester!(alix);
        tester!(bo);

        // Create a test oneshot message
        let readd_request = ReaddRequest {
            group_id: vec![1, 2, 3, 4],
            latest_commit_sequence_id: 0,
        };
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(readd_request)),
        };

        // Alix sends the oneshot message to Bo
        Oneshot::send_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
            .await
            .expect("Failed to send oneshot message");

        // Bo syncs welcomes to receive any oneshot groups
        bo.sync_welcomes().await.expect("Failed to sync welcomes");

        // Keep a stored one-shot: sender cleanup must not bypass the listing filter.
        MlsGroup::create_and_insert(
            alix.context.clone(),
            ConversationType::Oneshot,
            PreconfiguredPolicies::default().to_policy_set(),
            GroupMetadataOptions::default(),
            None,
        )?;
        let normal = alix.create_group(None, None)?;
        // Check that neither Alix nor Bo has any oneshot groups in find_groups
        let alix_groups = alix.find_groups(Default::default()).unwrap();
        let bo_groups = bo.find_groups(Default::default()).unwrap();

        // Oneshot groups should not appear in the regular groups list
        assert_eq!(alix_groups.len(), 1);
        assert_eq!(alix_groups[0].group_id, normal.group_id);
        assert_eq!(bo_groups.len(), 0, "Bo should have no groups");
    }

    #[tokio::test]
    async fn test_oneshot_groups_not_in_stream_groups() {
        tester!(alix);
        tester!(bo);

        // Subscribe to conversation events
        let mut alix_conversations = alix
            .stream_conversations(None, false)
            .await
            .expect("Failed to stream conversations");
        let mut bo_conversations = bo
            .stream_conversations(None, false)
            .await
            .expect("Failed to stream conversations");

        // Create a test oneshot message
        let readd_request = ReaddRequest {
            group_id: vec![5, 6, 7, 8],
            latest_commit_sequence_id: 0,
        };
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(readd_request)),
        };

        // Alix sends the oneshot message to Bo
        Oneshot::send_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
            .await
            .expect("Failed to send oneshot message");

        // Small delay to ensure any events would have been processed
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Try to get any conversation events with a timeout
        let alix_event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            alix_conversations.next(),
        )
        .await;
        let bo_event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            bo_conversations.next(),
        )
        .await;

        // Both should timeout (no new conversations)
        assert!(
            alix_event.is_err(),
            "Alix should not receive new conversation event"
        );
        assert!(
            bo_event.is_err(),
            "Bo should not receive new conversation event"
        );
    }

    #[tokio::test]
    async fn test_syncing_and_streaming_oneshot_group_simultaneously() {
        // Test syncing and streaming simultaneously, which causes Welcome to be processed twice
        // Note that on the second time, there is no group in the DB to refetch by ID - this should
        // not surface an error in the stream
        tester!(alix);
        tester!(bo);

        let mut bo_conversations = bo
            .stream_conversations(None, false)
            .await
            .expect("Failed to stream conversations");

        // Create a test oneshot message
        let readd_request = ReaddRequest {
            group_id: vec![5, 6, 7, 8],
            latest_commit_sequence_id: 0,
        };
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(readd_request)),
        };

        // Alix sends the oneshot message to Bo
        Oneshot::send_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
            .await
            .expect("Failed to send oneshot message");

        // Bo syncs welcomes to receive any oneshot groups
        bo.sync_welcomes().await.expect("Failed to sync welcomes");

        // Small delay to ensure any events would have been processed
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let bo_event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            bo_conversations.next(),
        )
        .await;

        assert!(
            bo_event.is_err(),
            "Bo should not receive new conversation event"
        );
    }
}
