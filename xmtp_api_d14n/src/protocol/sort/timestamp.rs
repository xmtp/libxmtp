use crate::protocol::{Envelope, Sort};

pub struct TimestampSort<'a, E> {
    envelopes: &'a mut [E],
}

impl<'b, 'a, E> Sort for TimestampSort<'b, E>
where
    E: Envelope<'a>,
{
    fn sort(mut self) {
        let envelopes = &mut self.envelopes;
        // we can only sort envelopes which have a timestamp
        envelopes.sort_unstable_by_key(|e| e.timestamp());
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
pub fn timestamp<'b, 'a: 'b, E: Envelope<'a>>(envelopes: &'b mut [E]) -> impl Sort {
    TimestampSort { envelopes }
}

#[cfg(test)]
mod tests {
    use crate::protocol::sort;
    use chrono::Utc;
    use proptest::prelude::*;
    use xmtp_common::Generate;
    use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
    use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

    use super::*;

    #[derive(Debug)]
    struct TestEnvelope {
        time: Option<chrono::DateTime<Utc>>,
    }

    impl TestEnvelope {
        fn new(time: i64) -> Self {
            Self {
                time: Some(chrono::DateTime::from_timestamp_nanos(time)),
            }
        }
    }

    impl Generate for TestEnvelope {
        fn generate() -> Self {
            TestEnvelope {
                time: Some(chrono::DateTime::from_timestamp_nanos(
                    xmtp_common::rand_i64(),
                )),
            }
        }
    }

    impl Envelope<'_> for TestEnvelope {
        fn topic(&self) -> Result<xmtp_proto::types::Topic, crate::protocol::EnvelopeError> {
            unreachable!()
        }

        fn payload(&self) -> Result<Payload, crate::protocol::EnvelopeError> {
            unreachable!()
        }

        fn timestamp(&self) -> Option<chrono::DateTime<Utc>> {
            self.time
        }

        fn client_envelope(&self) -> Result<ClientEnvelope, crate::protocol::EnvelopeError> {
            unreachable!()
        }

        fn group_message(
            &self,
        ) -> Result<Option<xmtp_proto::types::GroupMessage>, crate::protocol::EnvelopeError>
        {
            unreachable!()
        }

        fn welcome_message(
            &self,
        ) -> Result<Option<xmtp_proto::types::WelcomeMessage>, crate::protocol::EnvelopeError>
        {
            unreachable!()
        }

        fn consume<E>(&self, _extractor: E) -> Result<E::Output, crate::protocol::EnvelopeError>
        where
            Self: Sized,
            for<'a> crate::protocol::EnvelopeError:
                From<<E as crate::protocol::EnvelopeVisitor<'a>>::Error>,
            for<'a> E: crate::protocol::EnvelopeVisitor<'a> + crate::protocol::Extractor,
        {
            unreachable!()
        }
    }

    prop_compose! {
        fn envelope_all_some()(id in 1..u32::MAX) -> TestEnvelope {
            TestEnvelope::new(id as i64)
        }
    }

    fn is_sorted(sorted: &[TestEnvelope]) -> bool {
        sorted.is_sorted_by_key(|e| e.time)
    }

    #[xmtp_common::test]
    fn sorts_by_timestamp() {
        proptest!(|(mut envelopes in prop::collection::vec(envelope_all_some(), 0 .. 100))| {
            sort::timestamp(&mut envelopes).sort();
            assert!(is_sorted(&envelopes), "envelopes were not sorted")
        });
    }
}
