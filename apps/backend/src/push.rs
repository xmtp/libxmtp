//! Settled-envelope push delivery with a durable first-attempt cursor.

pub(crate) mod channel;
mod dispatcher;
mod suppress;
mod window;
mod work;

pub(crate) use dispatcher::PushHub;

/// The expiry of a recipient renewed at `renewed_ns`. An expiry that cannot be
/// represented is an invariant failure, not a client error.
pub(crate) fn recipient_expiry(
    config: &crate::config::push::PushConfig,
    renewed_ns: i64,
) -> Result<i64, crate::error::Error> {
    config
        .expires_at(renewed_ns)
        .ok_or(crate::error::Error::PushExpiryOverflow)
}

#[cfg(test)]
mod tests;
