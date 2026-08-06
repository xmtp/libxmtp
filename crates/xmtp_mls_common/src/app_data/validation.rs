use xmtp_proto::xmtp::mls::message_contents::MetadataPolicy as MetadataPolicyProto;
use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{
    Kind as MetadataPolicyKind, MetadataBasePolicy,
};

use super::component_id::ComponentId;
use super::component_registry::{ComponentOp, ComponentRegistry, ComponentRegistryError};

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
/// Carries everything needed to evaluate permissions for a single proposed
/// write: the component identity, the operation, the actor performing it,
/// and the raw old/new value bytes when available. Future change-aware base
/// policies will inspect the value bytes; today's evaluator only looks at
/// the actor.
///
/// `actor` is held by value (`ActorAuthority` is two booleans, smaller than
/// a pointer), so the lifetime `'a` only constrains the borrowed
/// `old_value` / `new_value` slices.
///
/// Construct via the generated builder so that `old_value` and `new_value`
/// (which share the same type) can't be accidentally swapped:
///
/// ```
/// # use xmtp_mls_common::app_data::component_id::ComponentId;
/// # use xmtp_mls_common::app_data::component_registry::ComponentOp;
/// # use xmtp_mls_common::app_data::validation::{ActorAuthority, ComponentChange};
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
/// 1. **Immutability**: Components in immutable ranges reject update and delete
///    unconditionally — only insert is allowed (and only if the component
///    doesn't exist yet, which the caller must verify).
/// 2. **Hardcoded**: The hardcoded components (component registry, super admin
///    list) have permissions enforced in code: super admin only.
/// 3. **Registry lookup**: All other components must have an entry in the
///    component registry. No entry = denied (deny by default).
pub fn validate_component_write(
    change: &ComponentChange<'_>,
    registry: &ComponentRegistry,
) -> Result<(), ComponentPermissionError> {
    let component_id = change.component_id;
    let op = change.op;

    // Layer 1: Immutability check
    if component_id.is_immutable() && matches!(op, ComponentOp::Update | ComponentOp::Delete) {
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
        return if change.actor.is_super_admin {
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

    match evaluate_policy_proto(&policy_proto, change) {
        PolicyOutcome::Allow => Ok(()),
        PolicyOutcome::Deny => Err(ComponentPermissionError::PermissionDenied(component_id, op)),
        PolicyOutcome::Invalid => Err(ComponentPermissionError::InvalidPolicy(component_id, op)),
    }
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

/// Walk a [`MetadataPolicyProto`] and evaluate it against the actor in a
/// [`ComponentChange`].
///
/// Currently only inspects `change.actor` (matching the existing
/// `MetadataBasePolicy` semantics). When change-aware base policies are added,
/// this function will inspect `change.old_value` / `change.new_value` for the
/// new variants — the field name → component ID mapping that the legacy
/// `MetadataFieldChange` carried lives in `change.component_id`.
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
fn evaluate_policy_proto(
    proto: &MetadataPolicyProto,
    change: &ComponentChange<'_>,
) -> PolicyOutcome {
    match &proto.kind {
        Some(MetadataPolicyKind::Base(base)) => evaluate_base_policy(*base, change.actor),
        Some(MetadataPolicyKind::AndCondition(and)) => {
            if and.policies.is_empty() {
                return PolicyOutcome::Invalid;
            }
            for inner in &and.policies {
                match evaluate_policy_proto(inner, change) {
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
                match evaluate_policy_proto(inner, change) {
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

fn evaluate_base_policy(base: i32, actor: ActorAuthority) -> PolicyOutcome {
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
        MetadataBasePolicy::Unspecified => PolicyOutcome::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::component_permissions::component_permissions;
    use crate::app_data::component_registry::new_component_metadata;
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentType, MetadataPolicy as MetadataPolicyProto,
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    };

    fn make_policy(base: MetadataBasePolicy) -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(base as i32)),
        }
    }

    fn allow() -> MetadataPolicyProto {
        make_policy(MetadataBasePolicy::Allow)
    }

    fn deny() -> MetadataPolicyProto {
        make_policy(MetadataBasePolicy::Deny)
    }

    fn admin_only() -> MetadataPolicyProto {
        make_policy(MetadataBasePolicy::AllowIfAdmin)
    }

    fn super_admin_only() -> MetadataPolicyProto {
        make_policy(MetadataBasePolicy::AllowIfSuperAdmin)
    }

    fn make_actor(is_admin: bool, is_super_admin: bool) -> ActorAuthority {
        ActorAuthority {
            is_admin,
            is_super_admin,
        }
    }

    /// Test helper. Constructs a [`ComponentChange`] with no value bytes.
    /// All current base policies only inspect the actor, so the value bytes
    /// don't affect any test outcome — they're intentionally untested at
    /// this layer until change-aware policies are added.
    fn change<'a>(id: ComponentId, op: ComponentOp, actor: ActorAuthority) -> ComponentChange<'a> {
        ComponentChange::builder()
            .component_id(id)
            .op(op)
            .actor(actor)
            .build()
    }

    fn member() -> ActorAuthority {
        make_actor(false, false)
    }

    fn admin() -> ActorAuthority {
        make_actor(true, false)
    }

    fn super_admin() -> ActorAuthority {
        make_actor(true, true)
    }

    fn setup_registry_with(
        id: ComponentId,
        insert: MetadataPolicyProto,
        update: MetadataPolicyProto,
        delete: MetadataPolicyProto,
    ) -> ComponentRegistry {
        let mut reg = ComponentRegistry::new();
        reg.set(
            id,
            new_component_metadata(
                component_permissions()
                    .insert(insert)
                    .update(update)
                    .delete(delete)
                    .call(),
                ComponentType::Bytes,
            ),
        )
        .unwrap();
        reg
    }

    // === Immutability Tests ===

    #[xmtp_common::test]
    fn test_immutable_insert_allowed() {
        let id = ComponentId::CONVERSATION_TYPE;
        let reg = setup_registry_with(id, allow(), deny(), deny());
        let actor = super_admin();
        let result = validate_component_write(&change(id, ComponentOp::Insert, actor), &reg);
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    fn test_immutable_update_rejected() {
        let id = ComponentId::CONVERSATION_TYPE;
        let reg = setup_registry_with(id, allow(), allow(), allow());
        let actor = super_admin();
        let result = validate_component_write(&change(id, ComponentOp::Update, actor), &reg);
        assert!(matches!(
            result,
            Err(ComponentPermissionError::ImmutableViolation(
                _,
                ComponentOp::Update
            ))
        ));
    }

    #[xmtp_common::test]
    fn test_immutable_delete_rejected() {
        let id = ComponentId::CONVERSATION_TYPE;
        let reg = setup_registry_with(id, allow(), allow(), allow());
        let actor = super_admin();
        let result = validate_component_write(&change(id, ComponentOp::Delete, actor), &reg);
        assert!(matches!(
            result,
            Err(ComponentPermissionError::ImmutableViolation(
                _,
                ComponentOp::Delete
            ))
        ));
    }

    // === Hardcoded Tests ===

    #[xmtp_common::test]
    fn test_registry_super_admin_allowed() {
        let reg = ComponentRegistry::new();
        let actor = super_admin();
        let result = validate_component_write(
            &change(ComponentId::COMPONENT_REGISTRY, ComponentOp::Update, actor),
            &reg,
        );
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    fn test_registry_admin_rejected() {
        let reg = ComponentRegistry::new();
        let actor = admin();
        let result = validate_component_write(
            &change(ComponentId::COMPONENT_REGISTRY, ComponentOp::Update, actor),
            &reg,
        );
        assert!(matches!(
            result,
            Err(ComponentPermissionError::SuperAdminRequired(_))
        ));
    }

    #[xmtp_common::test]
    fn test_registry_member_rejected() {
        let reg = ComponentRegistry::new();
        let actor = member();
        let result = validate_component_write(
            &change(ComponentId::COMPONENT_REGISTRY, ComponentOp::Update, actor),
            &reg,
        );
        assert!(matches!(
            result,
            Err(ComponentPermissionError::SuperAdminRequired(_))
        ));
    }

    #[xmtp_common::test]
    fn test_super_admin_list_super_admin_allowed() {
        let reg = ComponentRegistry::new();
        let actor = super_admin();
        let result = validate_component_write(
            &change(ComponentId::SUPER_ADMIN_LIST, ComponentOp::Insert, actor),
            &reg,
        );
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    fn test_super_admin_list_admin_rejected() {
        let reg = ComponentRegistry::new();
        let actor = admin();
        let result = validate_component_write(
            &change(ComponentId::SUPER_ADMIN_LIST, ComponentOp::Insert, actor),
            &reg,
        );
        assert!(matches!(
            result,
            Err(ComponentPermissionError::SuperAdminRequired(_))
        ));
    }

    #[xmtp_common::test]
    fn test_admin_list_with_admin_policy_admin_allowed() {
        let reg = setup_registry_with(
            ComponentId::ADMIN_LIST,
            admin_only(),
            admin_only(),
            admin_only(),
        );
        let actor = admin();
        let result = validate_component_write(
            &change(ComponentId::ADMIN_LIST, ComponentOp::Insert, actor),
            &reg,
        );
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    fn test_admin_list_with_admin_policy_member_rejected() {
        let reg = setup_registry_with(
            ComponentId::ADMIN_LIST,
            admin_only(),
            admin_only(),
            admin_only(),
        );
        let actor = member();
        let result = validate_component_write(
            &change(ComponentId::ADMIN_LIST, ComponentOp::Insert, actor),
            &reg,
        );
        assert!(matches!(
            result,
            Err(ComponentPermissionError::PermissionDenied(_, _))
        ));
    }

    #[xmtp_common::test]
    fn test_admin_list_with_super_admin_policy() {
        let reg = setup_registry_with(
            ComponentId::ADMIN_LIST,
            super_admin_only(),
            super_admin_only(),
            super_admin_only(),
        );
        let admin_actor = admin();
        let super_admin_actor = super_admin();
        // Admin rejected
        assert!(
            validate_component_write(
                &change(ComponentId::ADMIN_LIST, ComponentOp::Insert, admin_actor),
                &reg,
            )
            .is_err()
        );
        // Super admin allowed
        assert!(
            validate_component_write(
                &change(
                    ComponentId::ADMIN_LIST,
                    ComponentOp::Insert,
                    super_admin_actor,
                ),
                &reg,
            )
            .is_ok()
        );
    }

    // === Registry Lookup Tests ===

    #[xmtp_common::test]
    fn test_deny_by_default_no_entry() {
        let reg = ComponentRegistry::new();
        let actor = super_admin();
        let result = validate_component_write(
            &change(ComponentId::GROUP_NAME, ComponentOp::Insert, actor),
            &reg,
        );
        assert!(matches!(
            result,
            Err(ComponentPermissionError::NoRegistryEntry(_))
        ));
    }

    #[xmtp_common::test]
    fn test_insert_allow_policy() {
        let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), deny(), deny());
        let actor = member();
        let result = validate_component_write(
            &change(ComponentId::GROUP_NAME, ComponentOp::Insert, actor),
            &reg,
        );
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    fn test_update_admin_only_policy_admin_passes() {
        let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), admin_only(), deny());
        let actor = admin();
        let result = validate_component_write(
            &change(ComponentId::GROUP_NAME, ComponentOp::Update, actor),
            &reg,
        );
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    fn test_update_admin_only_policy_member_fails() {
        let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), admin_only(), deny());
        let actor = member();
        let result = validate_component_write(
            &change(ComponentId::GROUP_NAME, ComponentOp::Update, actor),
            &reg,
        );
        assert!(matches!(
            result,
            Err(ComponentPermissionError::PermissionDenied(
                _,
                ComponentOp::Update
            ))
        ));
    }

    #[xmtp_common::test]
    fn test_delete_deny_policy() {
        let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), allow(), deny());
        let actor = super_admin();
        let result = validate_component_write(
            &change(ComponentId::GROUP_NAME, ComponentOp::Delete, actor),
            &reg,
        );
        assert!(matches!(
            result,
            Err(ComponentPermissionError::PermissionDenied(
                _,
                ComponentOp::Delete
            ))
        ));
    }

    #[xmtp_common::test]
    fn test_delete_super_admin_only_policy() {
        let reg = setup_registry_with(
            ComponentId::GROUP_NAME,
            allow(),
            allow(),
            super_admin_only(),
        );
        let actor = super_admin();
        let result = validate_component_write(
            &change(ComponentId::GROUP_NAME, ComponentOp::Delete, actor),
            &reg,
        );
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    fn test_different_insert_vs_update_permissions() {
        // Mimics group membership: anyone can update, only admin can insert
        let reg = setup_registry_with(
            ComponentId::GROUP_MEMBERSHIP,
            admin_only(),
            allow(),
            admin_only(),
        );
        let member_actor = member();
        let admin_actor = admin();

        // Member can update
        assert!(
            validate_component_write(
                &change(
                    ComponentId::GROUP_MEMBERSHIP,
                    ComponentOp::Update,
                    member_actor,
                ),
                &reg,
            )
            .is_ok()
        );

        // Member cannot insert
        assert!(
            validate_component_write(
                &change(
                    ComponentId::GROUP_MEMBERSHIP,
                    ComponentOp::Insert,
                    member_actor,
                ),
                &reg,
            )
            .is_err()
        );

        // Admin can insert
        assert!(
            validate_component_write(
                &change(
                    ComponentId::GROUP_MEMBERSHIP,
                    ComponentOp::Insert,
                    admin_actor,
                ),
                &reg,
            )
            .is_ok()
        );
    }

    #[xmtp_common::test]
    fn test_app_range_component() {
        let app_id = ComponentId::new(0xC100);
        let reg = setup_registry_with(app_id, allow(), allow(), deny());
        let actor = member();
        let result = validate_component_write(&change(app_id, ComponentOp::Insert, actor), &reg);
        assert!(result.is_ok());
    }
}
