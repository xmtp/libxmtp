use prost::DecodeError as ProstDecodeError;
use thiserror::Error;
use vodozemac::{olm, DecodeError as VmacDecodeError};
use xmtp_proto::xmtp::v3::message_contents::{
    PadlockMessageEnvelope, PadlockMessageHeader, PadlockMessageSealedMetadata,
};

use crate::{
    storage::InboundMessage,
    types::{Address, InstallationId},
};

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("prostdecode:{0}")]
    ProstDecode(#[from] ProstDecodeError),
    #[error("vmacdecode:{0}")]
    VmacDecode(#[from] VmacDecodeError),
    #[error("error:{0}")]
    Generic(String),
}

pub fn decode_bytes<T: prost::Message + Default>(bytes: &[u8]) -> Result<T, PayloadError> {
    Ok(T::decode(bytes)?)
}

pub struct DecodedInboundMessage {
    pub sender_address: Address,
    pub sender_installation_id: InstallationId,
    pub recipient_address: Address,
    pub recipient_installation_id: InstallationId,
    pub is_prekey_message: bool,
    pub ciphertext: Vec<u8>,
    pub sent_at_ns: i64,
}

impl TryFrom<InboundMessage> for DecodedInboundMessage {
    type Error = PayloadError;

    fn try_from(value: InboundMessage) -> Result<Self, Self::Error> {
        let message_envelope: PadlockMessageEnvelope = decode_bytes(&value.payload)?;
        let message_header: PadlockMessageHeader = decode_bytes(&message_envelope.header_bytes)?;
        let unsealed_header: PadlockMessageSealedMetadata =
            decode_bytes(&message_header.sealed_metadata)?;

        Ok(Self {
            sender_address: unsealed_header.sender_user_address,
            sender_installation_id: unsealed_header.sender_installation_id,
            recipient_address: unsealed_header.recipient_user_address,
            recipient_installation_id: unsealed_header.recipient_installation_id,
            is_prekey_message: unsealed_header.is_prekey_message,
            ciphertext: message_envelope.ciphertext,
            sent_at_ns: value.sent_at_ns,
        })
    }
}

impl TryFrom<&DecodedInboundMessage> for olm::OlmMessage {
    type Error = PayloadError;

    fn try_from(value: &DecodedInboundMessage) -> Result<Self, Self::Error> {
        let olm_message = if value.is_prekey_message {
            olm::OlmMessage::PreKey(olm::PreKeyMessage::from_bytes(value.ciphertext.as_slice())?)
        } else {
            olm::OlmMessage::Normal(olm::Message::from_bytes(value.ciphertext.as_slice())?)
        };

        Ok(olm_message)
    }
}
