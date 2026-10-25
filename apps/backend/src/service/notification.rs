//! Ownership checks precede provider and subscription validation.

#[cfg(test)]
mod tests;
pub(crate) mod webhook_url;

use std::collections::HashSet;
use subtle::ConstantTimeEq;
use tonic::{Request, Response, Status};
use xmtp_configuration::MAX_SUBSCRIPTION_HMAC_KEYS;
use xmtp_cryptography::secret::Zeroizing;
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
const HMAC_KEY_BYTES: usize = 42;
pub(crate) const MALFORMED_REQUEST: &str = "request is malformed";
const MALFORMED_SUBSCRIPTION: &str = "subscription is malformed";

// implements: PUSH-253
#[tonic::async_trait]
impl api::notification_service_server::NotificationService for Backend {
    #[xmtp_common::rpc_span]
    async fn register(
        &self,
        request: Request<api::RegisterRequest>,
    ) -> Result<Response<api::RecipientState>, Status> {
        let mut request = request.into_inner();
        let hash = identity(&request.recipient_id, &request.recipient_secret)?;
        if let Some(recipient) = self.store.load_recipient(&request.recipient_id).await? {
            authenticate(&recipient, &hash)?;
        }
        let (channel, delivery, signing_key) = self.validate_delivery(&mut request).await?;
        let renewed_ns = self.store.clock_ns().await?;
        crate::push::recipient_expiry(&self.config.push, renewed_ns)?;
        let record = PushRecipientRecord {
            recipient_id: std::mem::take(&mut request.recipient_id),
            secret_hash: hash.to_vec(),
            channel,
            delivery,
            signing_key,
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
        let mut request = request.into_inner();
        let hash = identity(&request.recipient_id, &request.recipient_secret)?;
        let recipient = self
            .store
            .load_recipient(&request.recipient_id)
            .await?
            .ok_or(Error::PushRecipientMissing)?;
        authenticate(&recipient, &hash)?;
        let adds = validate_subscriptions(&mut request)?;
        let renewed_ns = self.store.clock_ns().await?;
        crate::push::recipient_expiry(&self.config.push, renewed_ns)?;
        let changes = self
            .store
            .apply_subscriptions(
                &request.recipient_id,
                &hash,
                adds,
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
            expires_at_ns: crate::push::recipient_expiry(&self.config.push, state.renewed_ns)?,
        })
    }

    /// Check channel availability, URL, and signing key in wire order. The
    /// signing key moves out of the request.
    // implements: PUSH-210, PUSH-256
    async fn validate_delivery(
        &self,
        request: &mut api::RegisterRequest,
    ) -> Result<(PushChannel, String, Option<Zeroizing<Vec<u8>>>), Status> {
        use api::register_request::Delivery;
        let unconfigured = || Status::failed_precondition("channel is not configured");
        let (channel, delivery, signing_key) = match request.delivery.as_mut() {
            Some(Delivery::Apns(apns)) => {
                self.config.push.apns.as_ref().ok_or_else(unconfigured)?;
                (PushChannel::Apns, apns.token.clone(), None)
            }
            Some(Delivery::Fcm(fcm)) => {
                self.config.push.fcm.as_ref().ok_or_else(unconfigured)?;
                (PushChannel::Fcm, fcm.token.clone(), None)
            }
            Some(Delivery::Http(http)) => {
                let signing_key = Zeroizing::new(std::mem::take(&mut http.signing_key));
                let config = self.config.push.http.as_ref().ok_or_else(unconfigured)?;
                webhook_url::validate(&http.url, config)
                    .await
                    .map_err(|_| Status::invalid_argument("webhook url is not allowed"))?;
                if !(MIN_SIGNING_KEY_BYTES..=MAX_SIGNING_KEY_BYTES).contains(&signing_key.len()) {
                    return Err(Status::invalid_argument(
                        "webhook signing key length is not allowed",
                    ));
                }
                (PushChannel::Http, http.url.clone(), Some(signing_key))
            }
            None => return Err(Status::invalid_argument(MALFORMED_REQUEST)),
        };
        if delivery.is_empty()
            || delivery.chars().count() > MAX_DELIVERY_CHARACTERS
            || delivery.contains('\0')
        {
            return Err(Status::invalid_argument(MALFORMED_REQUEST));
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
/// The HMAC keys move out of the request.
// implements: PUSH-215
fn validate_subscriptions(
    request: &mut api::UpdateSubscriptionsRequest,
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
        .iter_mut()
        .map(|add| {
            if add.hmac_epoch_base < 0
                || add.hmac_keys.len() > MAX_SUBSCRIPTION_HMAC_KEYS
                || add.hmac_keys.iter().any(|key| key.len() != HMAC_KEY_BYTES)
            {
                return Err(malformed());
            }
            let hmac_epoch_base = (!add.hmac_keys.is_empty()).then_some(add.hmac_epoch_base);
            let mut keys = add.hmac_keys.drain(..).map(Zeroizing::new);
            Ok(PushSubscriptionRecord {
                topic: add.topic.clone(),
                hmac_epoch_base,
                hmac_keys: std::array::from_fn(|_| keys.next()),
                include_commits: add.include_commits,
            })
        })
        .collect()
}
