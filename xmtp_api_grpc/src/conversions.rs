use prost::Message;
use xmtp_proto::xmtp::xmtpv4::{ClientEnvelope, PayerEnvelope, PublishEnvelopeRequest};

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
