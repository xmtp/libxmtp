use crate::types::Address;

pub trait Conversation {
    fn peer_address(&self) -> Address;
}

pub struct OneToOneConversation {
    peer_address: Address,
}

impl OneToOneConversation {
    pub fn new(peer_address: Address) -> Self {
        Self { peer_address }
    }
}

impl Conversation for OneToOneConversation {
    fn peer_address(&self) -> Address {
        self.peer_address.clone()
    }
}
