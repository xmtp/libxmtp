//! Push notification messages carry the recipient secret, the webhook signing
//! key and derived HMAC keys; a device-sync `HmacKeyUpdate` carries an HMAC
//! root key; group mutable metadata and the metadata-update intent can carry
//! the commit-log signer. Each message wipes those fields when it drops, so
//! every copy wipes itself: decoded requests, retry clones, the copies a
//! transport owns, and a value prost drops when a decode fails part way.
//! Buffers that prost and tonic encode into or copy through are not covered.

use crate::backend_v1::{
    HttpDelivery, RegisterRequest, Subscription, UnregisterRequest, UpdateSubscriptionsRequest,
};
use crate::xmtp::device_sync::content::HmacKeyUpdate;
use crate::xmtp::mls::database::send_message_data::V1 as SendMessageV1;
use crate::xmtp::mls::database::update_metadata_data::V1 as UpdateMetadataV1;
use crate::xmtp::mls::message_contents::GroupMutableMetadataV1;
use crate::xmtp::mls::message_contents::plaintext_envelope::V1 as PlaintextEnvelopeV1;
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

impl Drop for GroupMutableMetadataV1 {
    fn drop(&mut self) {
        // Attribute names are public; any value can be the commit-log signer.
        for value in self.attributes.values_mut() {
            value.zeroize();
        }
    }
}

impl Drop for UpdateMetadataV1 {
    fn drop(&mut self) {
        self.field_value.zeroize();
        self.expected_field_value.zeroize();
    }
}

impl Drop for SendMessageV1 {
    fn drop(&mut self) {
        self.payload_bytes.zeroize();
    }
}

impl Drop for PlaintextEnvelopeV1 {
    fn drop(&mut self) {
        self.content.zeroize();
    }
}
