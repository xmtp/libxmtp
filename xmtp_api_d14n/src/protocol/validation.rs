use super::{EnvelopeError, EnvelopeVisitor};
use std::collections::HashMap;
use thiserror::Error;
use xmtp_common::{RetryableError, time::now_ns};
use xmtp_proto::xmtp::{
    identity::associations::RecoverableEcdsaSignature,
    xmtpv4::envelopes::{
        OriginatorEnvelope, UnsignedOriginatorEnvelope, originator_envelope::Proof,
    },
};

const NS_IN_SEC: i64 = 1_000_000_000;
pub const NS_IN_HALF_HOUR: i64 = NS_IN_SEC * 60 * 30;

pub struct QueryEnvelopeValidator {
    originator_public_keys: HashMap<u32, Vec<u8>>,
    originator_proof: Option<RecoverableEcdsaSignature>,
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Message AAD is corrupt or invalid")]
    BadAAD,
    #[error("Invalid signature on {envelope:?} envelope")]
    InvalidSignature { envelope: EnvelopeType },
    #[error("{envelope:?} envelope in message is missing proof")]
    MissingProof { envelope: EnvelopeType },
    #[error("Received incorrect proof on {envelope:?} envelope in message. Found {found:?}")]
    IncorrectProof {
        envelope: EnvelopeType,
        found: Proof,
    },
    #[error("Time discrepancy is too great. ({0}ns)")]
    TimeDiscrepancy(i64),
}

impl RetryableError for ValidationError {
    fn is_retryable(&self) -> bool {
        false
    }
}

//impl From<ValidationError> for EnvelopeError {
//    fn from(err: ValidationError) -> Self {
//        EnvelopeError::Validation(err)
//    }
//}

#[derive(Debug)]
enum EnvelopeType {
    Originator,
}

impl EnvelopeVisitor<'_> for QueryEnvelopeValidator {
    type Error = ValidationError;

    fn visit_originator(&mut self, e: &OriginatorEnvelope) -> Result<(), Self::Error> {
        match &e.proof {
            Some(Proof::OriginatorSignature(ecdsa_sig)) => {
                self.originator_proof = Some(ecdsa_sig.clone());
                Ok(())
            }
            Some(found) => Err(ValidationError::IncorrectProof {
                envelope: EnvelopeType::Originator,
                found: found.clone(),
            }),
            None => Err(ValidationError::MissingProof {
                envelope: EnvelopeType::Originator,
            }),
        }
    }

    fn visit_unsigned_originator(
        &mut self,
        e: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        // Check the time discrepancy
        let time_diff = now_ns() - e.originator_ns;
        if time_diff > NS_IN_HALF_HOUR {
            return Err(ValidationError::TimeDiscrepancy(time_diff));
        }

        Ok(())
    }
}
