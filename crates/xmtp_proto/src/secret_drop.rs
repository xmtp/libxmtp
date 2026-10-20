//! Push notification messages carry the recipient secret, the webhook signing
//! key and derived HMAC keys. Each message wipes those fields when it drops, so
//! every copy wipes itself: decoded requests, retry clones and the copies a
//! transport owns. Buffers that prost and tonic encode into are not covered.

use crate::backend_v1::{
    HttpDelivery, RegisterRequest, Subscription, UnregisterRequest, UpdateSubscriptionsRequest,
};
use zeroize::Zeroize;

impl Drop for RegisterRequest {
    fn drop(&mut self) {
        self.recipient_secret.zeroize();
    }
}

impl Drop for HttpDelivery {
    fn drop(&mut self) {
        self.signing_key.zeroize();
    }
}

impl Drop for UnregisterRequest {
    fn drop(&mut self) {
        self.recipient_secret.zeroize();
    }
}

impl Drop for UpdateSubscriptionsRequest {
    fn drop(&mut self) {
        self.recipient_secret.zeroize();
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.hmac_keys.zeroize();
    }
}
