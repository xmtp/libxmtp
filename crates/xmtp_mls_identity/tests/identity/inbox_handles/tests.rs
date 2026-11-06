//! Handle tests through clients. Tests on the shared backend use random handles
//! and claim at most once per inbox, because a claimed handle is permanent
//! there. Tests that claim more than once use an ephemeral backend.

mod claims;
mod events;
mod resolver;
mod search;

pub(crate) use xmtp_mls::utils::test::inbox_handles::*;
