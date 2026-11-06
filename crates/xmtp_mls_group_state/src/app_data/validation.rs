use std::collections::HashSet;

use tls_codec::Deserialize;
use xmtp_proto::xmtp::mls::message_contents::ComponentType;
use xmtp_proto::xmtp::mls::message_contents::MetadataPolicy as MetadataPolicyProto;
use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{
    Kind as MetadataPolicyKind, MetadataBasePolicy,
};

use super::component_id::ComponentId;
use super::component_registry::{ComponentOp, ComponentRegistry, ComponentRegistryError};
use super::component_source::component_type;
use crate::inbox_id::InboxId;

/// The minimal subset of actor authority needed to evaluate base policies.
///
/// Carries only the booleans the policy evaluator inspects (admin and
/// super-admin status). This crate intentionally does not depend on the
/// richer `CommitParticipant` type from `xmtp_mls` — callers at the
/// integration boundary construct an `ActorAuthority` from whatever actor
/// representation they have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorAuthority {
    pub is_admin: bool,
    pub is_super_admin: bool,
}

/// A change being proposed against a component, used as the input to
/// [`validate_component_write`].
///
/// Carries everything a base policy may read: the
/// component, the element operation and key, the authenticated proposer
/// and its roles, and the membership after the commit. It never carries
/// the value being written into policy evaluation; `new_value` feeds only
/// component invariants.
///
/// `old_value` is the whole component before this proposal (`None` when
/// absent). Immutability reads it: an absent immutable component accepts
/// one first scalar `Update` or collection `Insert`s.
///
/// Construct via the generated builder so that `old_value` and `new_value`
/// (which share the same type) can't be accidentally swapped:
///
/// ```
/// # use xmtp_mls_group_state::app_data::component_id::ComponentId;
/// # use xmtp_mls_group_state::app_data::component_registry::ComponentOp;
/// # use xmtp_mls_group_state::app_data::validation::{ActorAuthority, ComponentChange};
/// # let actor = ActorAuthority { is_admin: false, is_super_admin: true };
/// # let old_bytes = vec![1, 2, 3];
/// # let new_bytes = vec![4, 5, 6];
/// let change = ComponentChange::builder()
///     .component_id(ComponentId::GROUP_NAME)
///     .op(ComponentOp::Update)
///     .actor(actor)
///     .old_value(&old_bytes)
///     .new_value(&new_bytes)
///     .build();
/// ```
#[derive(Debug, Clone, bon::Builder)]
pub struct ComponentChange<'a> {
    pub component_id: ComponentId,
    pub op: ComponentOp,
    pub actor: ActorAuthority,
    pub old_value: Option<&'a [u8]>,
    pub new_value: Option<&'a [u8]>,
    /// The authenticated proposer. `None` fails every self test closed.
    pub proposer: Option<InboxId>,
    /// Whether the proposer is one of the conversation's `DM_MEMBERS`.
    #[builder(default)]
    pub is_dm_participant: bool,
    /// TLS encoding of the element key for a map or registry mutation;
    /// `None` for a scalar, a set, or a whole-component Remove.
    pub key: Option<&'a [u8]>,
    /// Inbox ids in `GROUP_MEMBERSHIP` after the commit, or the committed
    /// membership for a standalone proposal. `None` fails every
    /// non-member test closed.
    pub membership: Option<&'a HashSet<InboxId>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ComponentPermissionError {
    #[error("immutable component {0} does not allow {1}")]
    ImmutableViolation(ComponentId, ComponentOp),
    #[error("component {0} requires super admin")]
    SuperAdminRequired(ComponentId),
    #[error("no registry entry for component {0}")]
    NoRegistryEntry(ComponentId),
    #[error("missing permissions in metadata for component {0}")]
    MissingPermissions(ComponentId),
    #[error("missing policy field for component {0} op {1}")]
    MissingPolicyField(ComponentId, ComponentOp),
    #[error("invalid policy for component {0} op {1}")]
    InvalidPolicy(ComponentId, ComponentOp),
    #[error("permission denied for component {0} op {1}")]
    PermissionDenied(ComponentId, ComponentOp),
    #[error("registry error: {0}")]
    RegistryError(#[from] ComponentRegistryError),
}

/// Validate whether the change's actor is allowed to perform the proposed
/// [`ComponentChange`].
///
/// Three-layer check:
/// 1. **Immutability**: a component in an immutable range rejects every
///    delete, and every write once it has a value. An absent
///    immutable component accepts its first write under its policy: a
///    scalar `Update` or collection `Insert`s. A map-element `Update`
///    could only rewrite a key the same delta inserted, so it is refused.
/// 2. **Hardcoded**: the component registry and the super admin list are
///    super admin only, except that a DM participant may change
///    application-range registry entries.
/// 3. **Registry lookup**: All other components must have an entry in the
///    component registry. No entry = denied (deny by default).
// implements: META-004, PERM-011, PERM-012, PERM-014, PERM-026
pub fn validate_component_write(
    change: &ComponentChange<'_>,
    registry: &ComponentRegistry,
) -> Result<(), ComponentPermissionError> {
    let component_id = change.component_id;
    let op = change.op;

    // Layer 1: Immutability check
    let first_write = change.old_value.is_none()
        && match op {
            ComponentOp::Insert => true,
            ComponentOp::Update => change.key.is_none(),
            ComponentOp::Delete => false,
        };
    if component_id.is_immutable() && !first_write {
        return Err(ComponentPermissionError::ImmutableViolation(
            component_id,
            op,
        ));
    }

    // Layer 2: Hardcoded components — always require super admin.
    // `is_hardcoded()` is the source of truth for which IDs land here:
    // adding a new hardcoded component is a single-line change to that
    // function and this branch picks it up automatically.
    if component_id.is_hardcoded() {
        return if change.actor.is_super_admin || is_dm_application_registry_change(change) {
            Ok(())
        } else {
            Err(ComponentPermissionError::SuperAdminRequired(component_id))
        };
    }

    // Layer 3: Registry lookup (deny by default)
    let meta = registry
        .get(&component_id)?
        .ok_or(ComponentPermissionError::NoRegistryEntry(component_id))?;

    // The registry's `validate_metadata` guarantees that any stored entry
    // has `permissions: Some` and all three policy fields populated. The
    // checks below are defensive — they should never fire in practice but
    // we'd rather return a structured error than panic.
    let permissions = meta
        .permissions
        .ok_or(ComponentPermissionError::MissingPermissions(component_id))?;

    let policy_proto: MetadataPolicyProto = match op {
        ComponentOp::Insert => permissions.insert_policy,
        ComponentOp::Update => permissions.update_policy,
        ComponentOp::Delete => permissions.delete_policy,
    }
    .ok_or(ComponentPermissionError::MissingPolicyField(
        component_id,
        op,
    ))?;

    // A well-known id has a fixed type; the registry types the rest.
    let component_type = component_type(component_id).map_or(meta.component_type, |ty| ty as i32);
    let inbox_keyed = [
        ComponentType::TlsMapInboxIdBytes as i32,
        ComponentType::TlsMapInboxIdString as i32,
    ]
    .contains(&component_type);
    let subject = PolicySubject {
        actor: ActorAuthority {
            // A DM participant satisfies ALLOW_IF_SUPER_ADMIN on
            // application components only.
            is_super_admin: change.actor.is_super_admin
                || (change.is_dm_participant && component_id.is_app_range()),
            ..change.actor
        },
        op,
        proposer: change.proposer,
        inbox_key: change
            .key
            .filter(|_| inbox_keyed)
            .and_then(|key| InboxId::tls_deserialize_exact(key).ok()),
        membership: change.membership,
    };
    match evaluate_policy_proto(&policy_proto, &subject) {
        PolicyOutcome::Allow => Ok(()),
        PolicyOutcome::Deny => Err(ComponentPermissionError::PermissionDenied(component_id, op)),
        PolicyOutcome::Invalid => Err(ComponentPermissionError::InvalidPolicy(component_id, op)),
    }
}

/// A DM participant may insert, update, or delete a registry
/// entry whose id is in the application range. Never `SUPER_ADMIN_LIST`,
/// never a well-known entry, never the whole registry.
fn is_dm_application_registry_change(change: &ComponentChange<'_>) -> bool {
    change.is_dm_participant
        && change.component_id == ComponentId::COMPONENT_REGISTRY
        && change
            .key
            .and_then(|key| ComponentId::tls_deserialize_exact(key).ok())
            .is_some_and(ComponentId::is_app_range)
}

/// Everything a base policy may read about one element change.
struct PolicySubject<'a> {
    /// The proposer's roles, with DM authority already applied.
    actor: ActorAuthority,
    op: ComponentOp,
    proposer: Option<InboxId>,
    /// The element key when the component is an inbox-keyed map.
    inbox_key: Option<InboxId>,
    membership: Option<&'a HashSet<InboxId>>,
}

/// Result of evaluating a [`MetadataPolicyProto`] against a [`ComponentChange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyOutcome {
    /// The policy permits the change.
    Allow,
    /// The policy denies the change.
    Deny,
    /// The policy is malformed (unknown base policy variant, empty combinator,
    /// missing kind, etc.).
    Invalid,
}

impl PolicyOutcome {
    fn from_bool(allowed: bool) -> Self {
        if allowed { Self::Allow } else { Self::Deny }
    }
}

/// Walk a [`MetadataPolicyProto`] and evaluate it against a
/// [`PolicySubject`].
///
/// **Combinator semantics:**
/// - `AndCondition` short-circuits on the first non-`Allow` outcome and
///   propagates it (so `Deny` or `Invalid` wins over later siblings).
/// - `AnyCondition` short-circuits on the first `Allow` *or* `Invalid`
///   outcome — a single malformed sub-policy poisons the whole `OR`. The
///   alternative ("keep scanning past `Invalid` looking for an `Allow`")
///   would let a sender hide a structurally broken policy as long as
///   *some* sibling allowed, which makes the broken policy invisible to
///   peers and harder to repair. Failing closed on `Invalid` is the
///   conservative choice.
/// - Empty `AndCondition` / `AnyCondition` are `Invalid` rather than
///   vacuously `Allow`/`Deny`.
// implements: PERM-007, PERM-008
fn evaluate_policy_proto(
    proto: &MetadataPolicyProto,
    subject: &PolicySubject<'_>,
) -> PolicyOutcome {
    match &proto.kind {
        Some(MetadataPolicyKind::Base(base)) => evaluate_base_policy(*base, subject),
        Some(MetadataPolicyKind::AndCondition(and)) => {
            if and.policies.is_empty() {
                return PolicyOutcome::Invalid;
            }
            for inner in &and.policies {
                match evaluate_policy_proto(inner, subject) {
                    PolicyOutcome::Allow => continue,
                    other => return other,
                }
            }
            PolicyOutcome::Allow
        }
        Some(MetadataPolicyKind::AnyCondition(any)) => {
            if any.policies.is_empty() {
                return PolicyOutcome::Invalid;
            }
            for inner in &any.policies {
                match evaluate_policy_proto(inner, subject) {
                    PolicyOutcome::Allow => return PolicyOutcome::Allow,
                    PolicyOutcome::Deny => {}
                    PolicyOutcome::Invalid => return PolicyOutcome::Invalid,
                }
            }
            PolicyOutcome::Deny
        }
        None => PolicyOutcome::Invalid,
    }
}

// implements: PERM-027, PERM-008
fn evaluate_base_policy(base: i32, subject: &PolicySubject<'_>) -> PolicyOutcome {
    let actor = subject.actor;
    let base = match MetadataBasePolicy::try_from(base) {
        Ok(b) => b,
        Err(_) => return PolicyOutcome::Invalid,
    };
    match base {
        MetadataBasePolicy::Allow => PolicyOutcome::Allow,
        MetadataBasePolicy::Deny => PolicyOutcome::Deny,
        MetadataBasePolicy::AllowIfAdmin => {
            PolicyOutcome::from_bool(actor.is_admin || actor.is_super_admin)
        }
        MetadataBasePolicy::AllowIfSuperAdmin => PolicyOutcome::from_bool(actor.is_super_admin),
        MetadataBasePolicy::AllowIfSelfOrNonMember => {
            // Denies a non-inbox-map type and a whole-component Remove,
            // neither of which has an inbox key.
            let Some(key) = subject.inbox_key else {
                return PolicyOutcome::Deny;
            };
            let is_self = subject.proposer == Some(key);
            PolicyOutcome::from_bool(match subject.op {
                ComponentOp::Insert | ComponentOp::Update => is_self,
                ComponentOp::Delete => {
                    is_self || subject.membership.is_some_and(|m| !m.contains(&key))
                }
            })
        }
        MetadataBasePolicy::Unspecified => PolicyOutcome::Invalid,
    }
}

#[cfg(test)]
mod tests;
