//! "Unpacked" variants of protobuf envelope types.
//!
//! The v4 XMTP protocol stores nested envelopes as raw `bytes` fields:
//! `OriginatorEnvelope → UnsignedOriginatorEnvelope → PayerEnvelope → ClientEnvelope`.
//! Traversing that hierarchy normally requires three `prost::Message::decode` calls, any of which can fail.
//!
//! These types reuse the **same protobuf wire format** but declare the nested bytes fields as
//! `message` fields at the same tag numbers.  Because both `bytes` and `message` share wire type 2
//! (length-delimited), `prost` will decode them inline and recursively in a single pass—no manual
//! decode calls required.
//!
//! ## Wire-format compatibility
//! `OriginatorEnvelope.unsigned_originator_envelope` is a `bytes` field at tag 1.
//! `UnpackedOriginatorEnvelope.unsigned_originator_envelope` is a `message` field at tag 1.
//! Both write `tag=1 | wt=2, length, <payload>` on the wire, so they are fully interchangeable.

use crate::xmtp::identity::associations::RecoverableEcdsaSignature;
use crate::xmtp::xmtpv4::envelopes::{ClientEnvelope, OriginatorEnvelope, originator_envelope};
use prost::Message;

/// [`PayerEnvelope`](crate::xmtp::xmtpv4::envelopes::PayerEnvelope) with `ClientEnvelope`
/// decoded inline (tag 1 was `bytes`, now `message`).
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnpackedPayerEnvelope {
    /// Decoded `ClientEnvelope` (was `unsigned_client_envelope: bytes` at tag 1).
    #[prost(message, optional, tag = "1")]
    pub unsigned_client_envelope: Option<ClientEnvelope>,
    #[prost(message, optional, tag = "2")]
    pub payer_signature: Option<RecoverableEcdsaSignature>,
    #[prost(uint32, tag = "3")]
    pub target_originator: u32,
    #[prost(uint32, tag = "4")]
    pub message_retention_days: u32,
}

/// [`UnsignedOriginatorEnvelope`](crate::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope)
/// with `PayerEnvelope` decoded inline (tag 4 was `bytes`, now `message`).
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnpackedUnsignedOriginatorEnvelope {
    #[prost(uint32, tag = "1")]
    pub originator_node_id: u32,
    #[prost(uint64, tag = "2")]
    pub originator_sequence_id: u64,
    #[prost(int64, tag = "3")]
    pub originator_ns: i64,
    /// Decoded `PayerEnvelope` (was `payer_envelope_bytes: bytes` at tag 4).
    #[prost(message, optional, tag = "4")]
    pub payer_envelope: Option<UnpackedPayerEnvelope>,
    #[prost(uint64, tag = "5")]
    pub base_fee_picodollars: u64,
    #[prost(uint64, tag = "6")]
    pub congestion_fee_picodollars: u64,
    #[prost(uint64, tag = "7")]
    pub expiry_unixtime: u64,
}

/// [`OriginatorEnvelope`](crate::xmtp::xmtpv4::envelopes::OriginatorEnvelope) with
/// `UnsignedOriginatorEnvelope` decoded inline (tag 1 was `bytes`, now `message`).
///
/// Reuses the existing [`originator_envelope::Proof`] oneof (tags 2 and 3 unchanged).
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnpackedOriginatorEnvelope {
    /// Decoded `UnsignedOriginatorEnvelope` (was `unsigned_originator_envelope: bytes` at tag 1).
    #[prost(message, optional, tag = "1")]
    pub unsigned_originator_envelope: Option<UnpackedUnsignedOriginatorEnvelope>,
    #[prost(oneof = "originator_envelope::Proof", tags = "2, 3")]
    pub proof: Option<originator_envelope::Proof>,
}

/// Same wire format as `QueryEnvelopesResponse` but yields `UnpackedOriginatorEnvelope`s.
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnpackedQueryEnvelopesResponse {
    #[prost(message, repeated, tag = "1")]
    pub envelopes: Vec<UnpackedOriginatorEnvelope>,
}

/// Same wire format as `SubscribeEnvelopesResponse` but yields `UnpackedOriginatorEnvelope`s.
#[derive(Clone, PartialEq, prost::Message)]
pub struct UnpackedSubscribeEnvelopesResponse {
    #[prost(message, repeated, tag = "1")]
    pub envelopes: Vec<UnpackedOriginatorEnvelope>,
}

impl TryFrom<&OriginatorEnvelope> for UnpackedOriginatorEnvelope {
    type Error = prost::DecodeError;

    /// Convert a packed [`OriginatorEnvelope`] to its unpacked form.
    ///
    /// Encodes the packed envelope and decodes as [`UnpackedOriginatorEnvelope`].
    /// Wire-format compatibility ensures nested bytes fields are decoded inline.
    fn try_from(packed: &OriginatorEnvelope) -> Result<Self, Self::Error> {
        Self::decode(packed.encode_to_vec().as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmtp::identity::associations::IdentityUpdate;
    use crate::xmtp::xmtpv4::envelopes::{
        AuthenticatedData, PayerEnvelope, UnsignedOriginatorEnvelope, client_envelope::Payload,
    };

    fn build_packed_originator() -> OriginatorEnvelope {
        let client = ClientEnvelope {
            aad: Some(AuthenticatedData {
                target_topic: vec![1, 2, 3],
                depends_on: None,
            }),
            payload: Some(Payload::IdentityUpdate(IdentityUpdate {
                actions: vec![],
                client_timestamp_ns: 42,
                inbox_id: "test".to_string(),
            })),
        };
        let payer = PayerEnvelope {
            unsigned_client_envelope: client.encode_to_vec(),
            payer_signature: None,
            target_originator: 7,
            message_retention_days: 30,
        };
        let unsigned = UnsignedOriginatorEnvelope {
            originator_node_id: 1,
            originator_sequence_id: 100,
            originator_ns: 999,
            payer_envelope_bytes: payer.encode_to_vec(),
            base_fee_picodollars: 0,
            congestion_fee_picodollars: 0,
            expiry_unixtime: 0,
        };
        OriginatorEnvelope {
            unsigned_originator_envelope: unsigned.encode_to_vec(),
            proof: None,
        }
    }

    #[xmtp_common::test]
    fn test_unpacked_originator_decodes_inline() {
        let packed = build_packed_originator();
        let unpacked = UnpackedOriginatorEnvelope::try_from(&packed).unwrap();

        let unsigned = unpacked.unsigned_originator_envelope.as_ref().unwrap();
        assert_eq!(unsigned.originator_node_id, 1);
        assert_eq!(unsigned.originator_sequence_id, 100);
        assert_eq!(unsigned.originator_ns, 999);

        let payer = unsigned.payer_envelope.as_ref().unwrap();
        assert_eq!(payer.target_originator, 7);
        assert_eq!(payer.message_retention_days, 30);

        let client = payer.unsigned_client_envelope.as_ref().unwrap();
        let aad = client.aad.as_ref().unwrap();
        assert_eq!(aad.target_topic, vec![1, 2, 3]);
    }

    #[xmtp_common::test]
    fn test_unpacked_roundtrip_wire_compat() {
        let packed = build_packed_originator();
        let packed_bytes = packed.encode_to_vec();

        // Decode packed bytes as unpacked — wire compat
        let unpacked = UnpackedOriginatorEnvelope::decode(packed_bytes.as_slice()).unwrap();
        // Re-encode the unpacked and decode as packed — still wire compat
        let repacked_bytes = unpacked.encode_to_vec();
        let repacked = OriginatorEnvelope::decode(repacked_bytes.as_slice()).unwrap();

        assert_eq!(
            packed.unsigned_originator_envelope,
            repacked.unsigned_originator_envelope
        );
    }

    #[xmtp_common::test]
    fn test_query_response_wire_compat() {
        use crate::xmtp::xmtpv4::message_api::QueryEnvelopesResponse;

        let packed = build_packed_originator();
        let response = QueryEnvelopesResponse {
            envelopes: vec![packed],
        };
        let bytes = response.encode_to_vec();

        let unpacked = UnpackedQueryEnvelopesResponse::decode(bytes.as_slice()).unwrap();
        assert_eq!(unpacked.envelopes.len(), 1);
        assert!(unpacked.envelopes[0].unsigned_originator_envelope.is_some());
    }
}
