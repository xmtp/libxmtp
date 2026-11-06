//! A stream's rent (RATE section 4): the `stream_rent` it has paid for the
//! current period and when the next period begins. Times are on the rate
//! limiter's clock, so a period and a retry delay count the same time.

use std::time::Duration;

/// How long one rent payment holds a stream's registrations.
const PERIOD: Duration = Duration::from_secs(60);
/// The topics each rent unit after the first pays for.
const TOPICS_PER_UNIT: usize = 1000;

/// A `stream_rent` spend a stream makes before the step it pays for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Due {
    /// The units to spend now.
    pub cost: usize,
    /// The whole rent of the topics the stream holds once it has paid: what
    /// a replacement stream pays for them from its open, so a denial reports
    /// the delay for this many units (RATE-032).
    pub whole: usize,
}

/// What one stream has paid for its current period, and when the next
/// begins. Every payment is recorded when it falls due; a denied payment
/// ends the stream, so nothing reads the record after one.
pub(super) struct Rent {
    /// Units paid for the current period.
    paid: usize,
    /// When the next period begins.
    renews_at: Duration,
}

impl Rent {
    /// The rent of a stream that opens at `now` holding `topics`, and the
    /// payment for its first period.
    pub fn open(now: Duration, topics: usize) -> (Self, Due) {
        let whole = rent(topics);
        let state = Self {
            paid: whole,
            renews_at: now.saturating_add(PERIOD),
        };
        (state, Due { cost: whole, whole })
    }

    /// When the next period begins.
    pub fn renews_at(&self) -> Duration {
        self.renews_at
    }

    /// The payment for a period that began by `now`, for the `topics` the
    /// stream holds. The periods follow one another from the open, so a
    /// stream that falls behind pays each one it reaches.
    pub fn renew(&mut self, now: Duration, topics: usize) -> Option<Due> {
        if now < self.renews_at {
            return None;
        }
        self.renews_at = self.renews_at.saturating_add(PERIOD);
        self.paid = rent(topics);
        Some(Due {
            cost: self.paid,
            whole: self.paid,
        })
    }

    /// The payment an update owes before it registers anything, when the
    /// `topics` it leaves registered raise the rent above what the stream
    /// paid for this period. An update that lowers the rent is not refunded.
    pub fn top_up(&mut self, topics: usize) -> Option<Due> {
        let whole = rent(topics);
        let cost = whole.checked_sub(self.paid).filter(|&cost| cost > 0)?;
        self.paid = whole;
        Some(Due { cost, whole })
    }
}

/// The rent of one period for a stream that holds `topics`.
fn rent(topics: usize) -> usize {
    1 + topics.div_ceil(TOPICS_PER_UNIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: Duration = Duration::from_secs(5);

    #[xmtp_common::test(unwrap_try = true)]
    fn one_unit_holds_the_stream_and_each_more_holds_a_thousand_topics() {
        for (topics, units) in [
            (0, 1),
            (1, 2),
            (1000, 2),
            (1001, 3),
            (2500, 4),
            (100_000, 101),
        ] {
            assert_eq!(
                Rent::open(OPEN, topics).1,
                Due {
                    cost: units,
                    whole: units
                }
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn an_update_pays_only_the_rent_it_raises_and_is_never_refunded() {
        let (mut rent, _) = Rent::open(OPEN, 900);
        assert_eq!(rent.top_up(1100), Some(Due { cost: 1, whole: 3 }));
        assert_eq!(rent.top_up(1200), None);
        assert_eq!(rent.top_up(10), None);
        // The stream already paid for 2000 topics in this period.
        assert_eq!(rent.top_up(2000), None);
        assert_eq!(rent.top_up(4500), Some(Due { cost: 3, whole: 6 }));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn each_period_from_the_open_pays_for_the_topics_held_then() {
        let (mut rent, _) = Rent::open(OPEN, 0);
        assert_eq!(rent.renews_at(), OPEN + Duration::from_secs(60));
        assert_eq!(
            rent.renew(OPEN + PERIOD - Duration::from_nanos(1), 2500),
            None
        );
        assert_eq!(
            rent.renew(OPEN + PERIOD, 2500),
            Some(Due { cost: 4, whole: 4 })
        );
        assert_eq!(rent.renews_at(), OPEN + PERIOD * 2);
        // A period's payment is what an update in it tops up from.
        assert_eq!(rent.top_up(2900), None);
        // A stream that fell two periods behind pays each in turn.
        let late = OPEN + PERIOD * 3;
        assert_eq!(rent.renew(late, 10), Some(Due { cost: 2, whole: 2 }));
        assert_eq!(rent.renew(late, 10), Some(Due { cost: 2, whole: 2 }));
        assert_eq!(rent.renew(late, 10), None);
        assert_eq!(rent.top_up(1001), Some(Due { cost: 1, whole: 3 }));
    }
}
