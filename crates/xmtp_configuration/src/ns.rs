//! Nanosecond units for the interval constants. They stay private so this
//! crate needs no dependency for them; `xmtp_common` exports the public ones.

pub(crate) const NS_IN_SEC: i64 = 1_000_000_000;
pub(crate) const NS_IN_HOUR: i64 = NS_IN_SEC * 60 * 60;
pub(crate) const NS_IN_DAY: i64 = NS_IN_HOUR * 24;
pub(crate) const NS_IN_30_DAYS: i64 = NS_IN_DAY * 30;

#[cfg(test)]
mod tests;
