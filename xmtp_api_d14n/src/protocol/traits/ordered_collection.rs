use xmtp_common::{MaybeSend, MaybeSync};

use crate::protocol::ResolutionError;

// these functions are not on `EnvelopeCollection` to keep its object-safety simpler.
// since dependency resolution requires `async fn`.
/// A ordered envelope collection
/// an `OrderedEnvelopeCollection` differs from [`Sort`](super::Sort)
/// since it adds the including of `async`, allowing
/// an `OrderedEnvelopeCollection`  to both
/// [Sort](super::Sort) and [ResolveDependencies](super::ResolveDependencies)
#[xmtp_common::async_trait]
pub trait OrderedEnvelopeCollection: MaybeSend + MaybeSync {
    /// Order dependencies of `Self` according to [XIP](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#335-cross-originator-message-ordering)
    async fn order(&mut self) -> Result<(), ResolutionError>;
}
