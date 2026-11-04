//! The context the engine runs on.

use crate::subscriptions::incoming::IncomingRuntime;
use xmtp_mls_core::context::CoreContext;
use xmtp_mls_identity::identity_updates::IdentityContext;

/// The engine's feature slot: the client's incoming runtime.
pub trait EngineFeatures {
    fn incoming(&self) -> &IncomingRuntime;
}

/// A context the engine runs on: core, identity resolution and the incoming
/// runtime.
pub trait XmtpSharedContext: IdentityContext + CoreContext<Features: EngineFeatures> {
    /// Shared incoming runtime. Its limits and transport are internal client policy.
    fn incoming_runtime(&self) -> &IncomingRuntime {
        self.features().incoming()
    }
}

impl<T: IdentityContext + CoreContext<Features: EngineFeatures>> XmtpSharedContext for T {}
