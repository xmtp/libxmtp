use prost::Message;
use xmtp_proto::xmtp::xmtpv4::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, PublishEnvelopeRequest,
    UnsignedOriginatorEnvelope,
};

pub fn wrap_client_envelope(req: ClientEnvelope) -> PublishEnvelopeRequest {
    let mut buf = vec![];
    req.encode(&mut buf).unwrap();

    PublishEnvelopeRequest {
        payer_envelope: Some(PayerEnvelope {
            unsigned_client_envelope: buf,
            payer_signature: None,
        }),
    }
}

pub fn extract_unsigned_originator_envelope(
    req: &OriginatorEnvelope,
) -> UnsignedOriginatorEnvelope {
    let mut unsigned_bytes = req.unsigned_originator_envelope.as_slice();
    UnsignedOriginatorEnvelope::decode(&mut unsigned_bytes)
        .expect("Failed to decode unsigned originator envelope")
}

pub fn extract_client_envelope(req: &OriginatorEnvelope) -> ClientEnvelope {
    let unsigned_originator = extract_unsigned_originator_envelope(req);

    let payer_envelope = unsigned_originator.payer_envelope.unwrap();
    let mut payer_bytes = payer_envelope.unsigned_client_envelope.as_slice();
    ClientEnvelope::decode(&mut payer_bytes).expect("Failed to decode client envelope")
}

pub fn extract_group_id_from_topic(topic: Vec<u8>) -> Vec<u8> {
    let topic_str = String::from_utf8(topic).expect("Failed to convert topic to string");
    let group_id = topic_str
        .split("/")
        .nth(1)
        .expect("Failed to extract group id from topic");
    group_id.as_bytes().to_vec()
}
