//! Ownership checks precede provider and subscription validation.

#[cfg(test)]
mod tests;
pub(crate) mod webhook_url;

use std::collections::HashSet;
use subtle::ConstantTimeEq;
use tonic::{Request, Response, Status};
use xmtp_proto::types::{Topic, TopicKind};

use crate::{
    Backend, api,
    db::{PushChannel, PushRecipientRecord, PushSubscriptionRecord, RecipientStateRecord},
    error::Error,
};

const IDENTITY_BYTES: usize = 32;
const MAX_DELIVERY_CHARACTERS: usize = 2048;
const MIN_SIGNING_KEY_BYTES: usize = 16;
const MAX_SIGNING_KEY_BYTES: usize = 64;
const MAX_METADATA_BYTES: usize = 4096;
const MAX_HMAC_KEYS: usize = 3;
const HMAC_KEY_BYTES: usize = 42;
pub(crate) const MALFORMED_REQUEST: &str = "request is malformed";
const MALFORMED_SUBSCRIPTION: &str = "subscription is malformed";

#[tonic::async_trait]
impl api::notification_service_server::NotificationService for Backend {
    #[xmtp_common::rpc_span]
    async fn register(
        &self,
        request: Request<api::RegisterRequest>,
    ) -> Result<Response<api::RecipientState>, Status> {
        let request = request.into_inner();
        let hash = identity(&request.recipient_id, &request.recipient_secret)?;
        if let Some(recipient) = self.store.load_recipient(&request.recipient_id).await? {
            authenticate(&recipient, &hash)?;
        }
        let (channel, delivery, signing_key) = self.validate_delivery(&request).await?;
        let renewed_ns = self.store.clock_ns().await?;
        self.config.push.expires_at(renewed_ns)?;
        let record = PushRecipientRecord {
            recipient_id: request.recipient_id,
            secret_hash: hash.to_vec(),
            channel,
            delivery,
            signing_key,
            metadata: request.metadata,
            renewed_ns,
        };
        let state = self.store.upsert_recipient(&record).await?;
        crate::telemetry::push_registered();
        Ok(Response::new(self.recipient_state(state)?))
    }

    #[xmtp_common::rpc_span]
    async fn unregister(
        &self,
        request: Request<api::UnregisterRequest>,
    ) -> Result<Response<api::UnregisterResponse>, Status> {
        let request = request.into_inner();
        let hash = identity(&request.recipient_id, &request.recipient_secret)?;
        let recipient = self
            .store
            .load_recipient(&request.recipient_id)
            .await?
            .ok_or(Error::PushRecipientMissing)?;
        authenticate(&recipient, &hash)?;
        if !self
            .store
            .delete_recipient(&request.recipient_id, &hash)
            .await?
        {
            return Err(Error::PushRecipientMissing.into());
        }
        crate::telemetry::push_unregistered();
        Ok(Response::new(api::UnregisterResponse {}))
    }

    #[xmtp_common::rpc_span]
    async fn update_subscriptions(
        &self,
        request: Request<api::UpdateSubscriptionsRequest>,
    ) -> Result<Response<api::RecipientState>, Status> {
        let request = request.into_inner();
        let hash = identity(&request.recipient_id, &request.recipient_secret)?;
        let recipient = self
            .store
            .load_recipient(&request.recipient_id)
            .await?
            .ok_or(Error::PushRecipientMissing)?;
        authenticate(&recipient, &hash)?;
        let adds = validate_subscriptions(&request)?;
        let renewed_ns = self.store.clock_ns().await?;
        self.config.push.expires_at(renewed_ns)?;
        let changes = self
            .store
            .apply_subscriptions(
                &request.recipient_id,
                &hash,
                &adds,
                &request.removes,
                self.config.limits.max_push_topics,
                renewed_ns,
            )
            .await?;
        crate::telemetry::push_subscriptions_changed(changes.added, changes.removed);
        Ok(Response::new(self.recipient_state(changes.state)?))
    }
}

impl Backend {
    /// Convert only the recipient row; a response never scans subscriptions.
    fn recipient_state(&self, state: RecipientStateRecord) -> Result<api::RecipientState, Error> {
        Ok(api::RecipientState {
            topic_count: state.topic_count as u64,
            channel: state.channel as i32,
            expires_at_ns: self.config.push.expires_at(state.renewed_ns)?,
        })
    }

    /// Check channel availability, URL, signing key, and metadata in wire order.
    async fn validate_delivery(
        &self,
        request: &api::RegisterRequest,
    ) -> Result<(PushChannel, String, Option<Vec<u8>>), Status> {
        use api::register_request::Delivery;
        let unconfigured = || Status::failed_precondition("channel is not configured");
        let (channel, delivery, signing_key) = match request.delivery.as_ref() {
            Some(Delivery::Apns(apns)) => {
                self.config.push.apns.as_ref().ok_or_else(unconfigured)?;
                (PushChannel::Apns, apns.token.clone(), None)
            }
            Some(Delivery::Fcm(fcm)) => {
                self.config.push.fcm.as_ref().ok_or_else(unconfigured)?;
                (PushChannel::Fcm, fcm.token.clone(), None)
            }
            Some(Delivery::Http(http)) => {
                let config = self.config.push.http.as_ref().ok_or_else(unconfigured)?;
                webhook_url::validate(&http.url, config)
                    .await
                    .map_err(|_| Status::invalid_argument("webhook url is not allowed"))?;
                if !(MIN_SIGNING_KEY_BYTES..=MAX_SIGNING_KEY_BYTES)
                    .contains(&http.signing_key.len())
                {
                    return Err(Status::invalid_argument(
                        "webhook signing key length is not allowed",
                    ));
                }
                (
                    PushChannel::Http,
                    http.url.clone(),
                    Some(http.signing_key.clone()),
                )
            }
            None => return Err(Status::invalid_argument(MALFORMED_REQUEST)),
        };
        if delivery.is_empty()
            || delivery.chars().count() > MAX_DELIVERY_CHARACTERS
            || delivery.contains('\0')
        {
            return Err(Status::invalid_argument(MALFORMED_REQUEST));
        }
        if request.metadata.len() > MAX_METADATA_BYTES {
            return Err(Status::invalid_argument("metadata is too large"));
        }
        Ok((channel, delivery, signing_key))
    }
}

/// Reject malformed identities before reading any recipient row.
fn identity(recipient_id: &[u8], secret: &[u8]) -> Result<[u8; IDENTITY_BYTES], Status> {
    if recipient_id.len() != IDENTITY_BYTES || secret.len() != IDENTITY_BYTES {
        return Err(Status::invalid_argument(MALFORMED_REQUEST));
    }
    Ok(xmtp_common::sha256_array(secret))
}

fn authenticate(recipient: &PushRecipientRecord, hash: &[u8]) -> Result<(), Status> {
    if !bool::from(recipient.secret_hash.as_slice().ct_eq(hash)) {
        return Err(Error::PushSecretInvalid.into());
    }
    Ok(())
}

/// Normalize the flat key slots and reject duplicate topics across both lists.
fn validate_subscriptions(
    request: &api::UpdateSubscriptionsRequest,
) -> Result<Vec<PushSubscriptionRecord>, Status> {
    let malformed = || Status::invalid_argument(MALFORMED_SUBSCRIPTION);
    let mut topics = HashSet::new();
    for bytes in request
        .adds
        .iter()
        .map(|add| &add.topic)
        .chain(request.removes.iter())
    {
        let topic = Topic::parse(bytes).map_err(|_| malformed())?;
        if !matches!(
            topic.kind(),
            TopicKind::GroupMessagesV1 | TopicKind::WelcomeMessagesV1
        ) || !topics.insert(topic)
        {
            return Err(malformed());
        }
    }
    request
        .adds
        .iter()
        .map(|add| {
            if add.hmac_epoch_base < 0
                || add.hmac_keys.len() > MAX_HMAC_KEYS
                || add.hmac_keys.iter().any(|key| key.len() != HMAC_KEY_BYTES)
            {
                return Err(malformed());
            }
            Ok(PushSubscriptionRecord {
                topic: add.topic.clone(),
                hmac_epoch_base: (!add.hmac_keys.is_empty()).then_some(add.hmac_epoch_base),
                hmac_keys: std::array::from_fn(|index| add.hmac_keys.get(index).cloned()),
                include_commits: add.include_commits,
            })
        })
        .collect()
}
