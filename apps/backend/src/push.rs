//! Settled-envelope push delivery with a durable first-attempt cursor.

pub(crate) mod channel;
mod dispatcher;
mod suppress;
mod window;
mod work;

pub(crate) use dispatcher::PushHub;

#[cfg(test)]
mod tests;
