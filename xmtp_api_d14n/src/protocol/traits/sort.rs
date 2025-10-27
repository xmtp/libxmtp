/// Envelopes in a d14n-context must be sorted according to its
/// dependencies, and by-originator.
/// [XIP, cross-originator sorting](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#335-cross-originator-message-ordering)
pub trait Sort {
    /// Sort envelopes in-place
    fn sort(self);
}
