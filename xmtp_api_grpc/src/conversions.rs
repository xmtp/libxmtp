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

pub fn extract_client_envelope(req: &OriginatorEnvelope) -> ClientEnvelope {
    let mut unsigned_bytes = req.unsigned_originator_envelope.as_slice();
    let unsigned_originator = UnsignedOriginatorEnvelope::decode(&mut unsigned_bytes)
        .expect("Failed to decode unsigned originator envelope");

    let payer_envelope = unsigned_originator.payer_envelope.unwrap();
    let mut payer_bytes = payer_envelope.unsigned_client_envelope.as_slice();
    ClientEnvelope::decode(&mut payer_bytes).expect("Failed to decode client envelope")
}
