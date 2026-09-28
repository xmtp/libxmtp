//! Durable send state across same-key retries, backend refusals, and
//! ambiguous publish failures.

use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use xmtp_db::group_message::DeliveryStatus;
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

type RefusingContext = Arc<
    crate::context::XmtpMlsLocalContext<
        RefusingApi,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

/// Fails the next queued group-message publishes with the given status, before
/// they reach the backend. Every other request passes through.
#[derive(Clone)]
struct RefusingApi {
    inner: crate::utils::TestClient,
    statuses: Arc<Mutex<VecDeque<tonic::Code>>>,
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for RefusingApi {
    type Error = ApiClientError;

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        let group_message = request
            .envelopes
            .iter()
            .any(|envelope| matches!(envelope.payload, Some(Payload::GroupMessage(_))));
        match group_message
            .then(|| self.statuses.lock().pop_front())
            .flatten()
        {
            Some(code) => Err(ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(tonic::Status::new(code, "refused")),
            )),
            None => self.inner.publish(request).await,
        }
    }

    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        self.inner.query(request).await
    }

    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        self.inner.query_newest(request).await
    }

    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        self.inner.get_inbox_ids(request).await
    }

    async fn get_configuration(
        &self,
        request: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, Self::Error> {
        self.inner.get_configuration(request).await
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }

    async fn register(
        &self,
        request: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.register(request).await
    }

    async fn unregister(
        &self,
        request: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.inner.unregister(request).await
    }

    async fn update_subscriptions(
        &self,
        request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.update_subscriptions(request).await
    }
}

/// The tester's group behind an API whose next group publishes fail with `statuses`.
async fn refusing_group(
    tester: &crate::utils::ClientTester,
    group_id: &GroupId,
    statuses: impl IntoIterator<Item = tonic::Code>,
) -> MlsGroup<RefusingContext> {
    let api = RefusingApi {
        inner: tester.context.api().api_client.clone(),
        statuses: Arc::new(Mutex::new(statuses.into_iter().collect())),
    };
    let client = crate::builder::ClientBuilder::from_client(tester.client.clone())
        .api_client(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await
        .unwrap();
    MlsGroup::new_cached(client.context.clone(), group_id)
        .unwrap()
        .0
}

fn intents_in<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    states: &[IntentState],
    kind: IntentKind,
) -> Vec<StoredGroupIntent> {
    group
        .context
        .db()
        .find_group_intents(group.group_id, Some(states.to_vec()), Some(vec![kind]))
        .unwrap()
}

const UNRESOLVED: [IntentState; 3] = [
    IntentState::ToPublish,
    IntentState::Published,
    IntentState::Committed,
];

fn status<C: XmtpSharedContext>(group: &MlsGroup<C>, message_id: &[u8]) -> DeliveryStatus {
    let message: StoredGroupMessage = group
        .context
        .db()
        .fetch(&message_id.to_vec())
        .unwrap()
        .unwrap();
    message.delivery_status
}

/// Resending the same content under the same idempotency key must
/// resolve to one stored message with at most one unresolved intent. After a
/// definite refusal, the retry must return the message to `Unpublished` in the
/// same transaction that queues the retry intent, so that the app never sees a
/// failed message with work pending; a published message is never queued again.
// verifies: SEND-003
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_same_key_reuses_one_message_and_one_unresolved_intent() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageStatusChanged]),
        Some(10),
    );
    let opts = || SendMessageOpts {
        idempotency_key: Some("same key".into()),
        ..Default::default()
    };
    let message_id = group.send_message_optimistic(b"same content", opts())?;
    assert_eq!(
        group.send_message_optimistic(b"same content", opts())?,
        message_id
    );
    let rows = group
        .find_messages(&Default::default())?
        .into_iter()
        .filter(|message| message.id == message_id)
        .count();
    assert_eq!(rows, 1, "a repeated key stored a second message");
    assert_eq!(
        intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).len(),
        1
    );

    let refusing = refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;
    assert!(refusing.publish_intents().await.is_err());
    assert_eq!(status(&group, &message_id), DeliveryStatus::Failed);
    events.drain();

    // Both entry points retry through the same writer.
    assert_eq!(
        group.send_message_optimistic(b"same content", opts())?,
        message_id
    );
    assert_eq!(status(&group, &message_id), DeliveryStatus::Unpublished);
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::MessageStatusChanged(change)), ..
        }] if change.message_id == message_id
            && change.previous == xmtp_events::MessageStatus::Failed
            && change.current == xmtp_events::MessageStatus::Unpublished
    ));
    group.send_message_optimistic(b"same content", opts())?;
    assert_eq!(
        intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).len(),
        1
    );
    assert_eq!(
        intents_in(&group, &[IntentState::Error], IntentKind::SendMessage).len(),
        1
    );

    group.publish_stored_message(&message_id).await?;
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
    assert_eq!(
        group.send_message_optimistic(b"same content", opts())?,
        message_id
    );
    group.publish_stored_message(&message_id).await?;
    assert!(
        intents_in(
            &group,
            &[IntentState::ToPublish, IntentState::Published],
            IntentKind::SendMessage
        )
        .is_empty()
    );
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
}

/// A backend `INVALID_ARGUMENT` proves the atomic publish stored
/// nothing, so no echo can arrive. The refused intent must end as `Error`, its
/// message as `Failed`, and later intents must publish in the same round,
/// including after a refused state change that would otherwise block the group.
// verifies: SEND-009
#[rstest::rstest]
#[case::message(IntentKind::SendMessage)]
#[case::state_change(IntentKind::KeyUpdate)]
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_definite_refusal_fails_both_records_and_releases_later_work(
    #[case] refused: IntentKind,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let refused_message = match refused {
        IntentKind::SendMessage => {
            Some(group.send_message_optimistic(b"refused", Default::default())?)
        }
        _ => {
            QueueIntent::key_update().queue(&group)?;
            None
        }
    };
    let later_id = group.send_message_optimistic(b"later work", Default::default())?;
    let refusing = refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;

    assert!(matches!(
        refusing.publish_intents().await,
        Err(GroupError::WrappedApi(xmtp_api::ApiError::Api(_)))
    ));
    let failed = intents_in(&group, &[IntentState::Error], refused);
    assert_eq!(failed.len(), 1, "the refused intent is not terminal");
    assert!(
        intents_in(
            &group,
            &[IntentState::ToPublish, IntentState::Published],
            refused
        )
        .iter()
        .all(|intent| intent.id != failed[0].id)
    );
    if let Some(id) = &refused_message {
        assert_eq!(status(&group, id), DeliveryStatus::Failed);
    }
    let later: StoredGroupMessage = group.context.db().fetch(&later_id)?.unwrap();
    assert!(
        later.envelope_hash.is_some(),
        "later work waited for an impossible echo"
    );

    assert!(
        group
            .sync_until_intent_resolved(failed[0].id)
            .await
            .is_err()
    );
    group.sync_until_last_intent_resolved().await?;
    assert_eq!(status(&group, &later_id), DeliveryStatus::Published);
    Ok(())
}

/// A status that can follow a committed publish is not a refusal.
/// The attempt must stay published with its exact bytes and no receipt, and
/// its message must stay `Unpublished`, so that the outcome can still settle.
// verifies: SEND-009
#[rstest::rstest]
#[case::out_of_range(tonic::Code::OutOfRange)]
#[case::permission_denied(tonic::Code::PermissionDenied)]
#[case::internal(tonic::Code::Internal)]
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_ambiguous_failure_keeps_the_attempt_pending(
    #[case] code: tonic::Code,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id = group.send_message_optimistic(b"ambiguous", Default::default())?;
    let (intent, attempt) = prepare_message(&group).await?;
    // Enough failures to outlast any retry of a retryable status.
    let refusing = refusing_group(&alix, &group.group_id, std::iter::repeat_n(code, 64)).await;

    assert!(refusing.publish_intents().await.is_err());
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Published);
    let saved =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    assert_eq!(saved, attempt);
    assert!(saved.receipts.is_none());
    assert_eq!(status(&group, &message_id), DeliveryStatus::Unpublished);
    Ok(())
}
