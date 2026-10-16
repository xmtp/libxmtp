use super::*;

// The private units must equal the public ones every other crate uses.
#[xmtp_common::test(unwrap_try = true)]
fn units_equal_the_shared_ones() {
    assert_eq!(NS_IN_SEC, xmtp_common::NS_IN_SEC);
    assert_eq!(NS_IN_HOUR, xmtp_common::NS_IN_HOUR);
    assert_eq!(NS_IN_DAY, xmtp_common::NS_IN_DAY);
    assert_eq!(NS_IN_30_DAYS, xmtp_common::NS_IN_30_DAYS);
}
