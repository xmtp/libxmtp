//! The features a client puts in its context.

use crate::attachments::{AttachmentFeatures, AttachmentRuntime};
use crate::context::LocalContext;
use crate::groups::context::EngineFeatures;
use crate::identity_updates::{IdentityFeatures, IdentityResolutionRegistry};
use crate::subscriptions::incoming::IncomingRuntime;
use std::sync::Arc;

/// Every feature of a client: attachments, the incoming runtime and identity
/// resolution.
#[derive(Clone, Default)]
pub struct MlsFeatures {
    pub(crate) attachments: Arc<AttachmentRuntime>,
    pub(crate) incoming: Arc<IncomingRuntime>,
    pub(crate) identity_resolutions: Arc<IdentityResolutionRegistry>,
}

/// The context of a client, with every feature the client has.
pub type XmtpMlsLocalContext<ApiClient, Db, S> = LocalContext<ApiClient, Db, S, MlsFeatures>;

impl AttachmentFeatures for MlsFeatures {
    fn attachments(&self) -> &Arc<AttachmentRuntime> {
        &self.attachments
    }
}

impl EngineFeatures for MlsFeatures {
    fn incoming(&self) -> &IncomingRuntime {
        &self.incoming
    }
}

impl IdentityFeatures for MlsFeatures {
    fn identity_resolutions(&self) -> &IdentityResolutionRegistry {
        &self.identity_resolutions
    }
}
