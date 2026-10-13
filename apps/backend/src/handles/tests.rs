use super::*;

fn handle(text: &str) -> Handle {
    Handle::parse(text).expect("the test names a valid handle")
}

/// A reserved name part is refused with or without a code, and a longer name
/// that starts with a reserved one is not reserved.
// verifies: API-297
#[xmtp_common::test(unwrap_try = true)]
fn reserved_name_part_is_not_allowed_with_any_code() {
    for reserved in RESERVED_NAMES {
        for text in [reserved.to_string(), format!("{reserved}#0001")] {
            assert_eq!(
                check_policy(&handle(&text), 1),
                Err(HandleRejection::NotAllowed),
                "{text}"
            );
        }
    }
    check_policy(&handle("adminx"), 1)?;
    check_policy(&handle("xadmin#0001"), 1)?;
}

/// The minimum counts the name part only, so a code does not lengthen a name.
// verifies: API-297
#[xmtp_common::test(unwrap_try = true)]
fn minimum_applies_to_the_name_part() {
    check_policy(&handle("abc"), 3)?;
    check_policy(&handle("abc#0001"), 3)?;
    for text in ["ab", "ab#0001"] {
        assert_eq!(
            check_policy(&handle(text), 3),
            Err(HandleRejection::NotAllowed),
            "{text}"
        );
    }
    let longest = "a".repeat(27);
    check_policy(&handle(&longest), 27)?;
}

/// A claim exactly 7 days after the previous one is admitted; one nanosecond
/// sooner is too soon. Any stored claim in the window counts, not only the
/// last in log order.
// verifies: API-298
#[xmtp_common::test(unwrap_try = true)]
fn interval_is_exactly_seven_days_of_server_time() {
    let clock = 10 * CLAIM_INTERVAL_NS;
    check_interval([], clock)?;
    check_interval([clock - CLAIM_INTERVAL_NS], clock)?;
    assert_eq!(
        check_interval([clock - CLAIM_INTERVAL_NS + 1], clock),
        Err(HandleRejection::TooSoon)
    );
    assert_eq!(
        check_interval([clock - 1, clock - 2 * CLAIM_INTERVAL_NS], clock),
        Err(HandleRejection::TooSoon)
    );
    assert_eq!(CLAIM_INTERVAL_NS, 604_800_000_000_000);
}

/// A name that is refused is reported as not allowed even when the claim is
/// also too soon.
#[xmtp_common::test(unwrap_try = true)]
fn name_is_checked_before_the_interval() {
    assert_eq!(
        check_claim(&handle("admin"), 1, [0], 1),
        Err(HandleRejection::NotAllowed)
    );
    assert_eq!(
        check_claim(&handle("nick"), 1, [0], 1),
        Err(HandleRejection::TooSoon)
    );
}
