use super::{GroupError, MlsGroup, PreconfiguredPolicies};
use crate::context::XmtpSharedContext;
use xmtp_common::snippet::Snippet;
use xmtp_db::{
    MlsProviderExt, XmtpMlsStorageProvider, group::ConversationType, prelude::QueryReaddStatus,
};
use xmtp_mls_common::{group::GroupMetadataOptions, group_metadata::GroupMetadata};
use xmtp_proto::xmtp::mls::message_contents::{OneshotMessage, oneshot_message};

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
    pub async fn send_message<C: XmtpSharedContext>(
        context: C,
        inbox_ids: Vec<&str>,
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
        if !inbox_ids.is_empty() {
            group.add_members_by_inbox_id(&inbox_ids).await?;
        }

        // Optional: delete group from DB here
        Ok(())
    }

    pub fn process_message(
        provider: &impl MlsProviderExt,
        _sender_inbox_id: String,
        sender_installation_id: Vec<u8>,
        message: OneshotMessage,
    ) -> Result<(), GroupError> {
        match message.message_type {
            Some(oneshot_message::MessageType::ReaddRequest(readd_request)) => {
                tracing::info!(
                    group_id = readd_request.group_id.snippet(),
                    sender_installation_id = sender_installation_id.snippet(),
                    latest_commit_sequence_id = readd_request.latest_commit_sequence_id,
                    "Received readd request for group"
                );
                provider.key_store().db().update_requested_at_sequence_id(
                    readd_request.group_id.as_slice(),
                    &sender_installation_id,
                    readd_request.latest_commit_sequence_id as i64,
                )?;
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
        id: u64,
        sender_inbox_id: String,
        sender_installation_id: Vec<u8>,
        metadata: GroupMetadata,
    ) -> Result<(), GroupError> {
        tracing::info!("Processing oneshot welcome");
        if let Some(message) = metadata.oneshot_message {
            Self::process_message(provider, sender_inbox_id, sender_installation_id, message)?;
        } else {
            tracing::warn!("Oneshot group welcome {} does not have oneshot message", id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use futures::stream::StreamExt;
    use xmtp_proto::xmtp::mls::message_contents::{ReaddRequest, oneshot_message::MessageType};

    #[tokio::test]
    async fn test_receive_oneshot_message_via_syncing() {
        use xmtp_db::prelude::QueryReaddStatus;

        tester!(alix);
        tester!(bo);
        tester!(caro);

        let group_id = vec![1, 2, 3, 4];
        let latest_commit_sequence_id = 42;

        // Verify that Bo and Caro have no readd status for Alix initially
        let bo_initial_status = bo
            .context
            .db()
            .get_readd_status(&group_id, alix.context.installation_id().as_slice())
            .expect("Failed to query readd status");
        assert!(
            bo_initial_status.is_none(),
            "Bo should not have readd status for Alix initially"
        );

        let caro_initial_status = caro
            .context
            .db()
            .get_readd_status(&group_id, alix.context.installation_id().as_slice())
            .expect("Failed to query readd status");
        assert!(
            caro_initial_status.is_none(),
            "Caro should not have readd status for Alix initially"
        );

        // Create a test oneshot message (using ReaddRequest as example)
        let readd_request = ReaddRequest {
            group_id: group_id.clone(),
            latest_commit_sequence_id,
        };
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(readd_request.clone())),
        };

        // Send the oneshot message
        Oneshot::send_message(
            alix.context.clone(),
            vec![bo.inbox_id(), caro.inbox_id()],
            oneshot_message.clone(),
        )
        .await
        .expect("Failed to send oneshot message");

        // Bo syncs welcomes
        bo.sync_welcomes().await.expect("Failed to sync welcomes");

        // Verify that Bo now has readd status for Alix with the correct sequence ID
        let bo_status = bo
            .context
            .db()
            .get_readd_status(&group_id, alix.context.installation_id().as_slice())
            .expect("Failed to query readd status")
            .expect("Bo should have readd status for Alix after syncing");

        assert_eq!(
            bo_status.requested_at_sequence_id,
            Some(latest_commit_sequence_id as i64),
            "Bo should have requested_at_sequence_id set to {}",
            latest_commit_sequence_id
        );
        assert_eq!(
            bo_status.responded_at_sequence_id, None,
            "Bo should not have responded_at_sequence_id set"
        );

        // Caro syncs welcomes
        caro.sync_welcomes().await.expect("Failed to sync welcomes");

        // Verify that Caro now has readd status for Alix with the correct sequence ID
        let caro_status = caro
            .context
            .db()
            .get_readd_status(&group_id, alix.context.installation_id().as_slice())
            .expect("Failed to query readd status")
            .expect("Caro should have readd status for Alix after syncing");

        assert_eq!(
            caro_status.requested_at_sequence_id,
            Some(latest_commit_sequence_id as i64),
            "Caro should have requested_at_sequence_id set to {}",
            latest_commit_sequence_id
        );
        assert_eq!(
            caro_status.responded_at_sequence_id, None,
            "Caro should not have responded_at_sequence_id set"
        );
    }

    #[tokio::test]
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

        // Check that neither Alix nor Bo has any oneshot groups in find_groups
        let alix_groups = alix.find_groups(Default::default()).unwrap();
        let bo_groups = bo.find_groups(Default::default()).unwrap();

        // Oneshot groups should not appear in the regular groups list
        assert_eq!(alix_groups.len(), 0, "Alix should have no groups");
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
