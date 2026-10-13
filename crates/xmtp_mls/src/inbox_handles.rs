//! Inbox handles on the client.
//!
//! A handle is a short public name that an inbox claims in its identity log
//! (`xmtp_id::associations::handle`). The client reads an inbox's handles
//! from the association states it derives. Storing a newer state reports a
//! changed display handle with the `handle.changed` event
//! (`identity_updates::store_association_state`), and group members carry the
//! handles of the newest state the client holds.

#[cfg(test)]
mod tests;
