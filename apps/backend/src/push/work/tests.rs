use super::*;
use crate::db::PushChannel;

fn delivery(id: u64, channel: PushChannel) -> Delivery {
    Delivery {
        payload: xmtp_push_types::PushPayload::new(&[1], id),
        config: DeliveryConfig {
            recipient_id: vec![id as u8; 32],
            channel,
            delivery: "https://example.org/push".into(),
            signing_key: Some(vec![7; 32]),
            metadata: vec![],
        },
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn cursor_waits_for_every_page_and_every_completed_first_attempt() {
    let mut work = Work::new(10);
    work.add_page(11, 20, true, vec![delivery(11, PushChannel::Http)]);
    let attempt = work.next(Instant::now())?;
    assert_eq!(work.low_watermark(), 10, "scheduling is not completion");
    assert!(matches!(
        work.complete(attempt, Outcome::Delivered, 3),
        Completion::Done("delivered")
    ));
    assert_eq!(work.low_watermark(), 10, "the next page is not loaded");
    work.add_page(11, 20, false, vec![delivery(20, PushChannel::Http)]);
    work.add_page(21, 30, false, vec![delivery(30, PushChannel::Apns)]);
    let first = work.next(Instant::now())?;
    let later = work.next(Instant::now())?;
    work.complete(later, Outcome::Delivered, 3);
    assert_eq!(work.low_watermark(), 10);
    work.complete(first, Outcome::Transient { retry_after: None }, 3);
    assert_eq!(work.low_watermark(), 30, "retries do not retain the cursor");
}

#[xmtp_common::test(unwrap_try = true)]
fn another_channel_and_later_windows_progress_past_saturated_permits() {
    let mut work = Work::new(0);
    work.add_page(
        1,
        1024,
        false,
        (1..=1024)
            .map(|id| delivery(id, PushChannel::Http))
            .collect(),
    );
    work.add_page(1025, 1025, false, vec![delivery(1025, PushChannel::Apns)]);
    let mut active = Vec::new();
    while let Some(attempt) = work.next(Instant::now()) {
        active.push(attempt);
    }
    assert_eq!(active.len(), CHANNEL_PERMITS + 1);
    assert_eq!(active.last()?.delivery.config.channel, PushChannel::Apns);
    assert_eq!(work.active, [1, 0, CHANNEL_PERMITS]);
    assert_eq!(work.low_watermark(), 0);
}

#[xmtp_common::test(unwrap_try = true)]
fn retry_delays_release_permits_and_clamp_provider_delay() {
    for (requested, expected) in [
        (Duration::ZERO, RETRY_DELAY),
        (Duration::from_secs(60), Duration::from_secs(60)),
        (Duration::from_secs(999), MAX_RETRY_DELAY),
    ] {
        let mut work = Work::new(0);
        work.add_page(1, 1, false, vec![delivery(1, PushChannel::Fcm)]);
        let attempt = work.next(Instant::now())?;
        let before = Instant::now();
        assert!(matches!(
            work.complete(
                attempt,
                Outcome::Transient {
                    retry_after: Some(requested)
                },
                3
            ),
            Completion::Retry
        ));
        assert_eq!(work.active, [0; 3]);
        let due = work.queue.front()?.due;
        assert!(due >= before + expected);
        assert!(work.next(due - Duration::from_nanos(1)).is_none());
        let retry = work.next(due)?;
        assert_eq!(retry.count, 2);
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn all_gone_is_terminal_but_one_unknown_outcome_keeps_the_recipient() {
    for mixed in [false, true] {
        let mut work = Work::new(0);
        work.add_page(1, 1, false, vec![delivery(1, PushChannel::Http)]);
        for index in 0..3 {
            let now = work.queue.front()?.due;
            let attempt = work.next(now)?;
            let outcome = if mixed && index == 1 {
                Outcome::Transient { retry_after: None }
            } else {
                Outcome::GoneTransient
            };
            let completed = work.complete(attempt, outcome, 3);
            if index < 2 {
                assert!(matches!(completed, Completion::Retry));
            } else if mixed {
                assert!(matches!(completed, Completion::Done("failed")));
            } else {
                assert!(matches!(completed, Completion::Dead(_)));
            }
        }
        assert!(work.is_empty());
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn gone_then_success_and_mismatch_do_not_delete() {
    let mut work = Work::new(0);
    work.add_page(1, 1, false, vec![delivery(1, PushChannel::Http)]);
    let first = work.next(Instant::now())?;
    work.complete(first, Outcome::GoneTransient, 3);
    let retry = work.next(work.queue.front()?.due)?;
    assert!(matches!(
        work.complete(retry, Outcome::Delivered, 3),
        Completion::Done("delivered")
    ));
    work.add_page(2, 2, false, vec![delivery(2, PushChannel::Apns)]);
    let first = work.next(Instant::now())?;
    assert!(matches!(
        work.complete(first, Outcome::Mismatch, 3),
        Completion::Done("mismatch")
    ));
    assert!(work.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
fn page_reservations_prevent_retries_from_exceeding_the_retained_bound() {
    let mut work = Work::new(0);
    work.add_page(1, 1, false, vec![delivery(1, PushChannel::Http)]);
    let first = work.next(Instant::now())?;
    work.add_page(
        2,
        2,
        false,
        (0..(RETAINED_LIMIT - super::super::window::PAGE_SIZE))
            .map(|_| delivery(2, PushChannel::Http))
            .collect(),
    );
    assert!(work.room_for_page());
    work.reserve_page();
    assert!(!work.room_for_page());
    assert!(matches!(
        work.complete(first, Outcome::Transient { retry_after: None }, 3),
        Completion::Done("failed")
    ));
    work.release_page();
    work.add_page(
        3,
        3,
        false,
        (0..super::super::window::PAGE_SIZE)
            .map(|_| delivery(3, PushChannel::Http))
            .collect(),
    );
    assert_eq!(work.queue.len(), RETAINED_LIMIT);
    for _ in 0..CHANNEL_PERMITS {
        let attempt = work.next(Instant::now())?;
        work.complete(attempt, Outcome::Delivered, 3);
    }
    assert!(work.queue.len() < RETAINED_LIMIT);
}

#[xmtp_common::test(unwrap_try = true)]
fn deletion_discards_only_matching_pending_configuration() {
    let mut work = Work::new(0);
    let old = delivery(1, PushChannel::Http);
    let mut renewed = old.clone();
    renewed.config.metadata = vec![9];
    work.add_page(1, 1, false, vec![old.clone(), renewed]);
    work.deleted(&old.config);
    assert_eq!(work.queue.len(), 1);
    assert_eq!(work.low_watermark(), 0);
    let pending = work.next(Instant::now())?;
    assert_eq!(pending.delivery.config.metadata, vec![9]);
    work.complete(pending, Outcome::Delivered, 3);
    assert_eq!(work.low_watermark(), 1);
}
