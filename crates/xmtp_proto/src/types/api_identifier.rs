use crate::xmtp::identity::associations::IdentifierKind;
use std::collections::HashMap;

/// Maps account addresses to inbox IDs. If no inbox ID found, the value will be None
pub type IdentifierToInboxIdMap = HashMap<ApiIdentifier, String>;
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ApiIdentifier {
    pub identifier: String,
    pub identifier_kind: IdentifierKind,
}
