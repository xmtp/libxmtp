//! compatibility decoding for v3 or d14n messages
//! we need this in case we can't tell whether bytes are v3 or d14n.
//! this will try to decode with v3 first, and if it erros will try to decode
//! with d14n. returns both errors on failure.
use prost::Message;
use std::fmt;
use xmtp_common::RetryableError;
use xmtp_proto::xmtp::mls::api::v1::GroupMessage as V3GroupMessage;
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessage as V3WelcomeMessage;
use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;

use crate::subscriptions::SubscribeError;

#[derive(thiserror::Error, Debug)]
enum D14nCompatDecodeError {
    #[error(
        "unable to decode externally streamed message `{}` for v3 or d14n\
        v3 errored with: {}\n\
        d14n errored with: {}\n",
        proto_type,
        v3,
        d14n
    )]
    FallbackFailure {
        v3: prost::DecodeError,
        d14n: prost::DecodeError,
        proto_type: &'static str,
    },
}

impl RetryableError for D14nCompatDecodeError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl D14nCompatDecodeError {
    fn err<T>(v3: prost::DecodeError, d14n: prost::DecodeError) -> SubscribeError {
        SubscribeError::dyn_err(D14nCompatDecodeError::FallbackFailure {
            v3,
            d14n,
            proto_type: std::any::type_name::<T>(),
        })
    }
}

#[derive(Debug)]
pub(crate) enum V3OrD14n<T> {
    D14n(OriginatorEnvelope),
    V3(T),
}

fn decode<T: prost::Message + Default + fmt::Debug>(
    bytes: &[u8],
) -> Result<V3OrD14n<T>, SubscribeError> {
    let v3 = T::decode(bytes);
    if let Ok(v3) = v3 {
        Ok(V3OrD14n::V3(v3))
    } else {
        let d14n = OriginatorEnvelope::decode(bytes);
        if let Ok(d14n) = d14n {
            Ok(V3OrD14n::D14n(d14n))
        } else {
            Err(D14nCompatDecodeError::err::<T>(
                v3.expect_err("checked for OK value"),
                d14n.expect_err("checked for OK value"),
            ))
        }
    }
}

/// Decode a welcome message from an opaque blob of bytes.
/// this should only be used if it is unknown whether the message is v3 or d14n.
/// this first tries to decode as a [`V3WelcomeMessage`]. If that fails,
/// it tries to decode as an [`OriginatorEnvelope`]. if that fails,
pub fn decode_welcome_message(bytes: &[u8]) -> Result<V3OrD14n<V3WelcomeMessage>, SubscribeError> {
    decode::<V3WelcomeMessage>(bytes)
}

/// Decode a group message from an opaque blob of bytes.
/// this should only be used if it is unknown whether the message is v3 or d14n.
/// this first tries to decode as a [`V3GroupMessage`]. If that fails,
/// it tries to decode as an [`OriginatorEnvelope`]. if that fails,
/// both decode errors are returned.
pub fn decode_group_message(bytes: &[u8]) -> Result<V3OrD14n<V3GroupMessage>, SubscribeError> {
    decode::<V3GroupMessage>(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use xmtp_proto::xmtp::xmtpv4::envelopes::originator_envelope;

    fn test_originator_envelope() -> OriginatorEnvelope {
        OriginatorEnvelope {
            unsigned_originator_envelope: vec![1, 2, 3],
            proof: Some(originator_envelope::Proof::OriginatorSignature(
                Default::default(),
            )),
        }
    }

    #[rstest]
    #[case::welcome_v3(
        decode_welcome_message,
        V3WelcomeMessage::default().encode_to_vec(),
        true,
    )]
    #[case::welcome_d14n(
        decode_welcome_message,
        test_originator_envelope().encode_to_vec(),
        false,
    )]
    #[case::group_v3(
        decode_group_message,
        V3GroupMessage::default().encode_to_vec(),
        true,
    )]
    #[case::group_d14n(
        decode_group_message,
        test_originator_envelope().encode_to_vec(),
        false,
    )]
    fn decode_compat_messages_table_driven<T>(
        #[case] decode_fn: fn(&[u8]) -> Result<V3OrD14n<T>, SubscribeError>,
        #[case] bytes: Vec<u8>,
        #[case] expect_v3: bool,
    ) where
        T: prost::Message + Default,
    {
        let decoded = decode_fn(&bytes).expect("message should decode");

        match (decoded, expect_v3) {
            (V3OrD14n::V3(decoded), true) => assert_eq!(decoded.encode_to_vec(), bytes),
            (V3OrD14n::D14n(decoded), false) => assert_eq!(decoded.encode_to_vec(), bytes),
            (V3OrD14n::V3(_), false) => panic!("expected d14n message"),
            (V3OrD14n::D14n(_), true) => panic!("expected v3 message"),
        }
    }
}
