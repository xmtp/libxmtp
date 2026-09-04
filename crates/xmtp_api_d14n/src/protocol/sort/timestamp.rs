use crate::protocol::{Envelope, EnvelopeError, Sort};

pub struct TimestampSort<'a, E> {
    envelopes: &'a mut [E],
}

impl<'b, 'a: 'b, E> Sort<()> for TimestampSort<'b, E>
where
    E: Envelope<'a>,
{
    fn sort(mut self) -> Result<Option<()>, EnvelopeError> {
        let envelopes = &mut self.envelopes;
        // we can only sort envelopes which have a timestamp
        envelopes.sort_unstable_by_key(|e| e.timestamp());
        // timestamp sort can never have missing dependencies
        Ok(None)
    }
}

/// Sorts Envelopes by server-side Timestamp in ascending order
/// * for d14n this will sort envelopes by
///   [`originator_ns`](xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope::originator_ns)
/// * for v3 this will sort by created_ns on GroupMessage, WelcomeMessage, or WelcomePointer
///   overall, sorts according to the timestamp extracted by
///   [`TimestampExtractor`](crate::protocol::TimestampExtractor)
///
/// If a timestamp does not have a cursor (extractor return [`Option::None`]) it is
/// sorted according to [`Ord`], [impl](https://doc.rust-lang.org/src/core/option.rs.html#2341)
/// This sort will never return any missing envelopes.
pub fn timestamp<'b, 'a: 'b, E: Envelope<'a>>(envelopes: &'b mut [E]) -> impl Sort<()> {
    TimestampSort { envelopes }
}
