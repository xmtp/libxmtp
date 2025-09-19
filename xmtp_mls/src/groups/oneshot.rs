use super::{GroupError, MlsGroup, PreconfiguredPolicies};
use crate::context::XmtpSharedContext;
use openmls::group::StagedWelcome;
use xmtp_db::group::ConversationType;
use xmtp_mls_common::{group::GroupMetadataOptions, group_metadata::GroupMetadata};
use xmtp_proto::xmtp::mls::message_contents::OneshotMessage;

impl<Context: XmtpSharedContext> MlsGroup<Context> {
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
    pub async fn send_oneshot_message(
        context: Context,
        inbox_ids: Vec<&str>,
        oneshot_message: OneshotMessage,
    ) -> Result<(), GroupError> {
        // Create a oneshot group with the oneshot message
        let group = Self::create_and_insert(
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

    pub fn process_oneshot_message(
        _context: Context,
        _message: OneshotMessage,
    ) -> Result<(), GroupError> {
        // TODO(rich): Handle oneshot message
        Ok(())
    }

    pub fn process_oneshot_welcome(
        context: Context,
        id: u64,
        _welcome: StagedWelcome,
        metadata: GroupMetadata,
    ) -> Result<(), GroupError> {
        tracing::info!("Processing oneshot welcome");
        if let Some(message) = metadata.oneshot_message {
            // TODO(rich): Extract welcome sender from StagedWelcome
            Self::process_oneshot_message(context, message)?;
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
        tester!(alix);
        tester!(bo);
        tester!(caro);

        // Create a test oneshot message (using ReaddRequest as example)
        let readd_request = ReaddRequest {
            group_id: vec![1, 2, 3, 4],
            latest_commit_sequence_id: 0,
        };
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(readd_request.clone())),
        };

        // Send the oneshot message
        MlsGroup::send_oneshot_message(
            alix.context.clone(),
            vec![bo.inbox_id(), caro.inbox_id()],
            oneshot_message.clone(),
        )
        .await
        .expect("Failed to send oneshot message");

        bo.sync_welcomes().await.expect("Failed to sync welcomes");
        // TODO(rich): Persist to DB when receiving oneshot message, then validate it is in the DB here
        // For now, just validate the message structure
        assert!(oneshot_message.message_type.is_some());
        if let Some(MessageType::ReaddRequest(request)) = oneshot_message.message_type {
            assert_eq!(request.group_id, vec![1, 2, 3, 4]);
        }
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
        MlsGroup::send_oneshot_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
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
        MlsGroup::send_oneshot_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
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
        MlsGroup::send_oneshot_message(alix.context.clone(), vec![bo.inbox_id()], oneshot_message)
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
