use crate::protocol::{ProtocolEnvelope, Sort};

pub struct TimestampSort<'a, E> {
    envelopes: &'a mut [E],
}

impl<'a, E> Sort for TimestampSort<'a, E>
where
    E: ProtocolEnvelope<'a>,
{
    fn sort(&mut self, topic_cursor: usize) {
        todo!()
    }
}
