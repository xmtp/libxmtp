//! Push notification messages carry the recipient secret, the webhook signing
//! key and derived HMAC keys, and a device-sync `HmacKeyUpdate` carries an HMAC
//! root key. Each message wipes those fields when it drops, so every copy
//! wipes itself: decoded requests, retry clones, the copies a transport owns,
//! and a value prost drops when a decode fails part way. Buffers that prost
//! and tonic encode into or copy through are not covered.

use crate::backend_v1::{
    HttpDelivery, RegisterRequest, Subscription, UnregisterRequest, UpdateSubscriptionsRequest,
};
use crate::xmtp::device_sync::content::HmacKeyUpdate;
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

impl Drop for HmacKeyUpdate {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
