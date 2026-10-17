pub mod update_dedupe;

/// implementations for some generated types
use crate::xmtp::mls::message_contents::{
    GroupUpdated, WelcomePointeeEncryptionAeadType, WelcomePointeeEncryptionAeadTypesExtension,
};
use std::hash::Hash;

impl Hash for GroupUpdated {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.initiated_by_inbox_id.hash(state);
        self.added_inboxes.hash(state);
        self.removed_inboxes.hash(state);
        self.metadata_field_changes.hash(state);
        self.left_inboxes.hash(state);
        self.added_admin_inboxes.hash(state);
        self.removed_admin_inboxes.hash(state);
        self.added_super_admin_inboxes.hash(state);
        self.removed_super_admin_inboxes.hash(state);
    }
}

impl WelcomePointeeEncryptionAeadTypesExtension {
    pub fn available_types() -> Self {
        Self {
            supported_aead_types: vec![WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into()],
        }
    }
}
