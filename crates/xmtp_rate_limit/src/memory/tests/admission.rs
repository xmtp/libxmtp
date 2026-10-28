use super::*;

/// The spec's admission rule, stated from its definitions rather than from the
/// limiter's TATs: each admitted spend is earned back at max(t, E′) + n × W / C
/// rounded down, and a spend is admitted when E + n × W / C − t ≤ W in exact
/// rationals for every limit, or n > C and E = t.
#[derive(Default)]
struct Rule {
    /// When the spends admitted so far under each limit are earned back.
    earned_back: [u128; crate::MAX_LIMITS],
}

impl Rule {
    fn admits(&self, limits: &[Limit], cost: u32, t: u64) -> bool {
        let (n, t) = (u128::from(cost), u128::from(t));
        n == 0
            || limits
                .iter()
                .zip(self.earned_back)
                .all(|(limit, earned_back)| {
                    let (c, w) = (u128::from(limit.count), limit.window.as_nanos());
                    let e = earned_back.max(t);
                    // (E − t) + n × W / C ≤ W, multiplied through by C.
                    (e - t) * c + n * w <= w * c || (n > c && e == t)
                })
    }

    fn charge(&mut self, limits: &[Limit], cost: u32, t: u64) {
        if cost == 0 {
            return;
        }
        for (limit, earned_back) in limits.iter().zip(&mut self.earned_back) {
            let (c, w) = (u128::from(limit.count), limit.window.as_nanos());
            *earned_back = (*earned_back).max(u128::from(t)) + u128::from(cost) * w / c;
        }
    }
}

// verifies: RATE-020
#[xmtp_common::test]
fn admits_exactly_what_the_admission_rule_admits() {
    let ops = proptest::collection::vec((advance_strategy(), 0u32..=120), 1..120);
    runner()
        .run(&(limits_strategy(1..=3), ops), |(limits, ops)| {
            let (limiter, clock) = limiter();
            let mut rule = Rule::default();
            for (advance, cost) in ops {
                clock.advance(advance);
                let now = clock.now();
                let expected = rule.admits(&limits, cost, now);
                let decision = spend(&limiter, &limits, cost);
                prop_assert_eq!(
                    decision == Decision::Allowed,
                    expected,
                    "cost {} at {}",
                    cost,
                    now
                );
                if expected {
                    rule.charge(&limits, cost, now);
                }
            }
            Ok(())
        })
        .unwrap();
}

/// The comparison is exact when C does not divide n × W: a spend the added
/// time rounded down would fit, but the exact added time does not, is denied
/// until the nanosecond the exact time fits.
#[xmtp_common::test]
fn the_admission_check_is_exact_below_a_nanosecond() {
    // 3 per 10 ns: three units at 0 leave a TAT of 9 ns, and at 2 ns a fourth
    // needs 7 + 10/3 ns.
    let tiny = [Limit {
        count: 3,
        window: Duration::from_nanos(10),
    }];
    // 7 per minute, after a burst of 7, 1 ns before the eighth fits.
    let minute = [limit(7, 60)];
    for (limits, burst, denied_at) in [(&tiny, 3, 2), (&minute, 7, 8_571_428_568)] {
        let (limiter, clock) = limiter();
        let mut rule = Rule::default();
        for _ in 0..burst {
            assert_eq!(spend(&limiter, limits, 1), Decision::Allowed);
            rule.charge(limits, 1, 0);
        }
        clock.set(denied_at);
        assert!(!rule.admits(limits, 1, denied_at));
        assert_eq!(
            spend(&limiter, limits, 1),
            Decision::Denied {
                retry_after: Duration::from_millis(1)
            }
        );
        clock.set(denied_at + 1);
        assert!(rule.admits(limits, 1, denied_at + 1));
        assert_eq!(spend(&limiter, limits, 1), Decision::Allowed);
    }
}

/// A spend of 2C waits until an earlier spend is earned back, is then
/// admitted, and is charged in full: the next single unit waits W + W/C.
#[xmtp_common::test]
fn an_oversized_spend_waits_for_full_allowance_and_pays_in_full() {
    const COUNT: u32 = 10;
    const WINDOW: Duration = Duration::from_secs(60);
    const UNIT: Duration = Duration::from_secs(6); // W / C
    let limits = [Limit {
        count: COUNT,
        window: WINDOW,
    }];
    let (limiter, clock) = limiter();

    assert_eq!(spend(&limiter, &limits, 1), Decision::Allowed);
    assert_eq!(
        spend(&limiter, &limits, 2 * COUNT),
        Decision::Denied { retry_after: UNIT }
    );
    clock.set(UNIT.as_nanos() as u64 - 1);
    assert!(matches!(
        spend(&limiter, &limits, 2 * COUNT),
        Decision::Denied { .. }
    ));

    clock.set(UNIT.as_nanos() as u64);
    assert_eq!(spend(&limiter, &limits, 2 * COUNT), Decision::Allowed);

    let wait = WINDOW + UNIT;
    assert_eq!(
        spend(&limiter, &limits, 1),
        Decision::Denied { retry_after: wait }
    );
    let admitted_at = UNIT + wait;
    clock.set((admitted_at - Duration::from_millis(1)).as_nanos() as u64);
    assert!(matches!(
        spend(&limiter, &limits, 1),
        Decision::Denied { .. }
    ));
    clock.set(admitted_at.as_nanos() as u64);
    assert_eq!(spend(&limiter, &limits, 1), Decision::Allowed);
}

/// A burst of `count` single units fits one window even when W / C is not a
/// whole number of nanoseconds, and the next unit is denied.
#[xmtp_common::test]
fn a_full_burst_fits_its_window() {
    let limits = [Limit {
        count: 7,
        window: Duration::from_nanos(1_000_000_003),
    }];
    let (limiter, _clock) = limiter();
    for _ in 0..7 {
        assert_eq!(spend(&limiter, &limits, 1), Decision::Allowed);
    }
    assert!(matches!(
        spend(&limiter, &limits, 1),
        Decision::Denied { .. }
    ));
}

/// A cost of 0 is admitted and charges nothing, even for a caller whose
/// oversized spend put it more than a window past now.
#[xmtp_common::test]
fn a_zero_cost_is_admitted_without_a_charge() {
    let limits = [limit(1, 60)];
    let (limiter, _clock) = limiter();

    assert_eq!(spend(&limiter, &limits, 0), Decision::Allowed);
    assert_eq!(tats(&limiter), None);

    assert_eq!(spend(&limiter, &limits, 5), Decision::Allowed);
    let charged = tats(&limiter);
    assert_eq!(charged, Some([300 * SECOND, 0, 0]));
    assert_eq!(spend(&limiter, &limits, 0), Decision::Allowed);
    assert_eq!(tats(&limiter), charged);
}

#[xmtp_common::test]
fn no_limits_admit_every_spend() {
    let (limiter, _clock) = limiter();
    for cost in [1, u32::MAX, u32::MAX] {
        assert_eq!(spend(&limiter, &[], cost), Decision::Allowed);
    }
    assert_eq!(tats(&limiter), None);
}

/// Categories and caller keys keep separate charges.
#[xmtp_common::test]
fn each_caller_and_category_has_its_own_allowance() {
    let limits = [limit(1, 60)];
    let (limiter, _clock) = limiter();
    assert_eq!(spend_as(&limiter, b"a", 1, &limits, 1), Decision::Allowed);
    assert!(matches!(
        spend_as(&limiter, b"a", 1, &limits, 1),
        Decision::Denied { .. }
    ));
    assert_eq!(spend_as(&limiter, b"a", 2, &limits, 1), Decision::Allowed);
    assert_eq!(spend_as(&limiter, b"b", 1, &limits, 1), Decision::Allowed);
    assert_eq!(spend_as(&limiter, b"", 1, &limits, 1), Decision::Allowed);
}

/// The largest cost, window, and time neither overflow nor panic; TATs and
/// delays saturate.
#[xmtp_common::test]
fn extreme_values_saturate() {
    let limits = [Limit {
        count: 1,
        window: Duration::MAX,
    }];
    let (limiter, clock) = limiter();
    assert_eq!(spend(&limiter, &limits, u32::MAX), Decision::Allowed);
    assert_eq!(tats(&limiter), Some([u64::MAX, 0, 0]));
    assert_eq!(
        spend(&limiter, &limits, 1),
        Decision::Denied {
            retry_after: Duration::from_millis(u64::MAX.div_ceil(MILLI))
        }
    );

    clock.set(u64::MAX);
    assert_eq!(spend(&limiter, &limits, u32::MAX), Decision::Allowed);
    let max = [Limit {
        count: u32::MAX,
        window: Duration::MAX,
    }];
    assert_eq!(
        spend_as(&limiter, b"other", 0, &max, u32::MAX),
        Decision::Allowed
    );
}

/// A zero window would earn back any cost at once. Debug builds stop on it;
/// release builds admit every spend under it, as an unbounded rate would.
#[xmtp_common::test]
fn a_zero_window_panics_in_debug_and_admits_in_release() {
    let (limiter, _clock) = limiter();
    let limits = [Limit {
        count: 1,
        window: Duration::ZERO,
    }];
    let first =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| spend(&limiter, &limits, 5)));
    if cfg!(debug_assertions) {
        assert!(first.is_err());
        assert_eq!(tats(&limiter), None);
    } else {
        assert_eq!(first.ok(), Some(Decision::Allowed));
        assert_eq!(spend(&limiter, &limits, 5), Decision::Allowed);
    }
}

/// A count of 0 behaves as a count of 1 instead of dividing by zero.
#[xmtp_common::test]
fn a_zero_count_counts_as_one() {
    let (limiter, _clock) = limiter();
    let limits = [limit(0, 60)];
    assert_eq!(spend(&limiter, &limits, 1), Decision::Allowed);
    assert_eq!(
        spend(&limiter, &limits, 1),
        Decision::Denied {
            retry_after: Duration::from_secs(60)
        }
    );
}

/// Limits after the third have no slot to be recorded in. Debug builds stop on
/// such a list; release builds ignore the extra limits.
#[xmtp_common::test]
fn more_than_three_limits_panic_in_debug_and_are_cut_in_release() {
    let (limiter, _clock) = limiter();
    let limits = [limit(5, 60), limit(5, 120), limit(5, 180), limit(1, 240)];
    let first =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| spend(&limiter, &limits, 1)));
    if cfg!(debug_assertions) {
        assert!(first.is_err());
        assert_eq!(tats(&limiter), None);
    } else {
        assert_eq!(first.ok(), Some(Decision::Allowed));
        assert_eq!(spend(&limiter, &limits, 1), Decision::Allowed);
    }
}
