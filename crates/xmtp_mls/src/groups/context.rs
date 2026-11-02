//! The context the engine runs on.

use crate::context::CoreContext;
use crate::identity_updates::IdentityContext;
use crate::subscriptions::incoming::IncomingRuntime;

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
