use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

// verifies: RATE-022
#[xmtp_common::test]
fn a_denied_spend_charges_no_limit() {
    let denials = AtomicUsize::new(0);
    let ops = proptest::collection::vec((advance_strategy(), 0u32..=120), 1..120);
    runner()
        .run(&(limits_strategy(1..=3), ops), |(limits, ops)| {
            let (limiter, clock) = limiter();
            for (advance, cost) in ops {
                clock.advance(advance);
                let before = tats(&limiter);
                if let Decision::Denied { .. } = spend(&limiter, &limits, cost) {
                    denials.fetch_add(1, Ordering::Relaxed);
                    prop_assert_eq!(tats(&limiter), before);
                }
            }
            Ok(())
        })
        .unwrap();
    assert!(denials.into_inner() > 0, "no case was denied");
}

/// A spend one limit admits and a later limit denies leaves the first limit
/// uncharged too.
#[xmtp_common::test]
fn a_denial_by_one_limit_leaves_the_others_uncharged() {
    let limits = [limit(100, 3600), limit(2, 60)];
    let (limiter, _clock) = limiter();
    assert_eq!(spend(&limiter, &limits, 1), Decision::Allowed);
    assert_eq!(spend(&limiter, &limits, 1), Decision::Allowed);
    let charged = tats(&limiter);
    assert_eq!(charged, Some([72 * SECOND, 60 * SECOND, 0]));

    for _ in 0..5 {
        assert_eq!(
            spend(&limiter, &limits, 1),
            Decision::Denied {
                retry_after: Duration::from_secs(30)
            }
        );
    }
    assert_eq!(tats(&limiter), charged);
}

// verifies: RATE-024
#[xmtp_common::test]
fn the_retry_delay_is_the_first_millisecond_the_spend_is_admitted() {
    let denials = AtomicUsize::new(0);
    let ops = proptest::collection::vec((advance_strategy(), 1u32..=120), 1..120);
    runner()
        .run(&(limits_strategy(1..=3), ops), |(limits, ops)| {
            let (limiter, clock) = limiter();
            for (advance, cost) in ops {
                clock.advance(advance);
                let Decision::Denied { retry_after } = spend(&limiter, &limits, cost) else {
                    continue;
                };
                denials.fetch_add(1, Ordering::Relaxed);
                prop_assert!(retry_after >= Duration::from_millis(1));
                prop_assert_eq!(retry_after.subsec_nanos() % MILLI as u32, 0);
                let denied_at = clock.now();
                let delay = retry_after.as_nanos() as u64;
                clock.set(denied_at + delay - MILLI);
                prop_assert!(
                    matches!(spend(&limiter, &limits, cost), Decision::Denied { .. }),
                    "admitted 1 ms before the delay"
                );
                clock.set(denied_at + delay);
                prop_assert_eq!(spend(&limiter, &limits, cost), Decision::Allowed);
            }
            Ok(())
        })
        .unwrap();
    assert!(denials.into_inner() > 0, "no case was denied");
}
