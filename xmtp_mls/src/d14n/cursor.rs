//! Decentralization Cursor Type.
//!
//! A Decentralized cursor is made up of two components
//! Originator Node Id: ID Identifying the node a message "Originated" on.
//! Sequence Id: Global Sequence ID for all items on a node
//! V3 items are categorized with a constant cursor, according to
//! [`xmtp_configuration::Originators`](Originators)


/// A d14n cursor
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cursor {
    sequence_id: u64,
    originator_id: u16
}

impl Cursor {

}
