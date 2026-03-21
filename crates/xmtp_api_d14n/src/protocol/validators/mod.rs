use xmtp_proto::{
    ConversionError,
    types::TopicKind,
    xmtp::xmtpv4::envelopes::{
        ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
    },
};

use crate::protocol::{EnvelopeVisitor, ProtocolEnvelope as _};

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("the originator envelope is expired. the recorded timestamp is older than 30 minutes")]
    OriginatorEnvelopeExpired { now_ns: i64, originator_ns: i64 },
    #[error("the payer signature is not valid")]
    InvalidPayerSignature,
    #[error("a client envelope must have authenticated data")]
    ClientEnvelopeAuthenticatedDataMissing,
    #[error(transparent)]
    Conversion(#[from] ConversionError),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EnvelopeValidator;

impl<'env> EnvelopeVisitor<'env> for EnvelopeValidator {
    type Error = ValidationError;

    fn visit_originator(&mut self, _e: &OriginatorEnvelope) -> Result<(), Self::Error> {
        // TODO verify orginator signature
        Ok(())
    }

    fn visit_unsigned_originator(
        &mut self,
        e: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        const MINS_30: i64 = 1_800_000_000_000;
        let now_ns = xmtp_common::time::now_ns();
        if e.originator_ns < now_ns.saturating_sub(MINS_30) {
            return Err(ValidationError::OriginatorEnvelopeExpired {
                now_ns,
                originator_ns: e.originator_ns,
            });
        }

        // TODO assert originator_sequence_id increments

        Ok(())
    }

    fn visit_payer(&mut self, _e: &PayerEnvelope) -> Result<(), Self::Error> {
        // TODO reconstruct pubkey from e.payer_signature
        // TODO compare the pubkey with an expected key

        // TODO he enclosed PayerEnvelope matches the originally published PayerEnvelope.

        Ok(())
    }

    fn visit_client(&mut self, e: &ClientEnvelope) -> Result<(), Self::Error> {
        if e.get_nested()?.is_some() {
            let aad = e
                .aad
                .as_ref()
                .filter(|a| !a.target_topic.is_empty())
                .ok_or(ValidationError::ClientEnvelopeAuthenticatedDataMissing)?;

            let (kind, bytes) = aad.target_topic.split_at(1);
            let kind = TopicKind::try_from(kind[0])?;

            // TODO topic bytes must match its kind
            let _ = (kind, bytes);

            // TODO fetch a GlobalCursor?
            // Envelope::depends_on?
            // The blockchain sequence IDs in the cursor are equal to the latest blockchain
            // payloads published on the topic. If not, a 409 status code must be returned with the
            // server’s current cursor.
        }

        Ok(())
    }
}
