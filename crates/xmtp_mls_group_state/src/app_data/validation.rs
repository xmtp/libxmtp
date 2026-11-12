use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::mem::MaybeUninit;

use tls_codec::Deserialize;
use xmtp_cryptography::secret::Zeroize;
use xmtp_proto::xmtp::mls::message_contents::MetadataPolicy as MetadataPolicyProto;
use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{
    Kind as MetadataPolicyKind, MetadataBasePolicy,
};
use xmtp_proto::xmtp::mls::message_contents::{ComponentMetadata, ComponentType};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
/// [`ComponentChange`], as [`ComponentWriteValidator::validate`] does.
///
/// It decodes the component's registry entry for this one change. To judge
/// many changes against one registry, keep one [`ComponentWriteValidator`].
pub fn validate_component_write(
    change: &ComponentChange<'_>,
    registry: &ComponentRegistry,
) -> Result<(), ComponentPermissionError> {
    ComponentWriteValidator::new(registry).validate(change)
}

/// Judges writes against one [`ComponentRegistry`], such as every change of
/// one commit.
///
/// It decodes each component's registry entry once. It evaluates each
/// policy at most once per operation and subject: the proposer's roles, and
/// whether the change is to the proposer's own key or a non-member's key.
/// The registry is borrowed, so it cannot change while an entry or a
/// verdict is reused.
pub struct ComponentWriteValidator<'r> {
    registry: &'r ComponentRegistry,
    /// Each component's recognized registry entry; `None` when absent.
    entries: HashMap<ComponentId, CachedEntry>,
    /// The verdict of each component's policy for an operation and subject.
    verdicts: HashMap<(ComponentId, ComponentOp, PolicySubject), PolicyOutcome>,
}

impl<'r> ComponentWriteValidator<'r> {
    pub fn new(registry: &'r ComponentRegistry) -> Self {
        Self {
            registry,
            entries: HashMap::new(),
            verdicts: HashMap::new(),
        }
    }

    /// The registry this validator judges against.
    pub fn registry(&self) -> &'r ComponentRegistry {
        self.registry
    }

    /// The recognized registry entry of `id`, decoded on first use.
    pub(crate) fn entry(
        &mut self,
        id: ComponentId,
    ) -> Result<Option<&ComponentMetadata>, ComponentRegistryError> {
        cached_entry(&mut self.entries, self.registry, id)
    }

    /// Validate whether the change's actor is allowed to perform the
    /// proposed [`ComponentChange`].
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
    pub fn validate(
        &mut self,
        change: &ComponentChange<'_>,
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
        let meta = cached_entry(&mut self.entries, self.registry, component_id)?
            .ok_or(ComponentPermissionError::NoRegistryEntry(component_id))?;

        // The registry's `validate_metadata` guarantees that any stored entry
        // has `permissions: Some` and all three policy fields populated. The
        // checks below are defensive — they should never fire in practice but
        // we'd rather return a structured error than panic.
        let permissions = meta
            .permissions
            .as_ref()
            .ok_or(ComponentPermissionError::MissingPermissions(component_id))?;

        let policy_proto = match op {
            ComponentOp::Insert => &permissions.insert_policy,
            ComponentOp::Update => &permissions.update_policy,
            ComponentOp::Delete => &permissions.delete_policy,
        }
        .as_ref()
        .ok_or(ComponentPermissionError::MissingPolicyField(
            component_id,
            op,
        ))?;

        // A well-known id has a fixed type; the registry types the rest.
        let component_type =
            component_type(component_id).map_or(meta.component_type, |ty| ty as i32);
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
            self_or_non_member: is_self_or_non_member(change, inbox_keyed),
        };
        // The registry fixes the policy of a component and operation, and
        // the subject holds everything else a base policy reads.
        let outcome = *self
            .verdicts
            .entry((component_id, op, subject))
            .or_insert_with(|| evaluate_policy_proto(policy_proto, &subject));
        match outcome {
            PolicyOutcome::Allow => Ok(()),
            PolicyOutcome::Deny => {
                Err(ComponentPermissionError::PermissionDenied(component_id, op))
            }
            PolicyOutcome::Invalid => {
                Err(ComponentPermissionError::InvalidPolicy(component_id, op))
            }
        }
    }
}

/// The recognized registry entry of `id`, decoded from `registry` into
/// `entries` on first use.
fn cached_entry<'e>(
    entries: &'e mut HashMap<ComponentId, CachedEntry>,
    registry: &ComponentRegistry,
    id: ComponentId,
) -> Result<Option<&'e ComponentMetadata>, ComponentRegistryError> {
    let entry = match entries.entry(id) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert(CachedEntry::new(registry.get(&id)?)),
    };
    Ok(entry.get())
}

/// A decoded registry entry on the heap, wiped when dropped.
///
/// A decoded `ComponentMetadata` has unwritten bytes: the tail of each
/// `None` field and of each short `oneof` variant. Moving it from the stack
/// to the heap copies the dead stack bytes under those tails, which can hold
/// a secret such as the commit-log signer, into the heap block. Dropping the
/// entry zeroes every byte of it before its block is freed. It is boxed so a
/// map that grows moves only the pointer and frees no copy of the entry.
struct CachedEntry(Box<MaybeUninit<Option<ComponentMetadata>>>);

impl CachedEntry {
    fn new(entry: Option<ComponentMetadata>) -> Self {
        Self(Box::new(MaybeUninit::new(entry)))
    }

    fn get(&self) -> Option<&ComponentMetadata> {
        // SAFETY: `new` initializes the value, and only `drop` drops it.
        unsafe { self.0.assume_init_ref() }.as_ref()
    }
}

impl Drop for CachedEntry {
    fn drop(&mut self) {
        // SAFETY: `new` initialized the value, and nothing reads it after
        // this drop.
        unsafe { self.0.assume_init_drop() };
        self.0.zeroize();
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

/// Whether `ALLOW_IF_SELF_OR_NON_MEMBER` allows the change: an insert or
/// update of the proposer's own key, or a delete of its own key or of a key
/// not in the membership.
// implements: PERM-027
fn is_self_or_non_member(change: &ComponentChange<'_>, inbox_keyed: bool) -> bool {
    // Denies a non-inbox-map type and a whole-component Remove,
    // neither of which has an inbox key.
    let Some(key) = change
        .key
        .filter(|_| inbox_keyed)
        .and_then(|key| InboxId::tls_deserialize_exact(key).ok())
    else {
        return false;
    };
    let is_self = change.proposer == Some(key);
    match change.op {
        ComponentOp::Insert | ComponentOp::Update => is_self,
        ComponentOp::Delete => is_self || change.membership.is_some_and(|m| !m.contains(&key)),
    }
}

/// Everything a base policy reads about one element change. A verdict is
/// reused for every change with the same component, operation, and subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PolicySubject {
    /// The proposer's roles, with DM authority already applied.
    actor: ActorAuthority,
    /// Whether `ALLOW_IF_SELF_OR_NON_MEMBER` allows the change.
    self_or_non_member: bool,
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
fn evaluate_policy_proto(proto: &MetadataPolicyProto, subject: &PolicySubject) -> PolicyOutcome {
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

/// Evaluate one base policy. It reads only `subject`, so a verdict keyed by
/// the component, operation, and subject can be reused.
// implements: PERM-027, PERM-008
fn evaluate_base_policy(base: i32, subject: &PolicySubject) -> PolicyOutcome {
    #[cfg(any(test, feature = "test-utils"))]
    record_work(|work| work.base_policy_evaluations += 1);
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
            PolicyOutcome::from_bool(subject.self_or_non_member)
        }
        MetadataBasePolicy::Unspecified => PolicyOutcome::Invalid,
    }
}

/// Registry decodes and base policy evaluations, counted for cost tests.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ValidationWork {
    /// Registry entries decoded.
    pub registry_decodes: u64,
    /// Base policies evaluated: one for each leaf of a policy tree walked,
    /// whether or not a verdict is reused.
    pub base_policy_evaluations: u64,
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static WORK: std::cell::Cell<ValidationWork> = const {
        std::cell::Cell::new(ValidationWork {
            registry_decodes: 0,
            base_policy_evaluations: 0,
        })
    };
}

/// Run `f` and return the registry decodes and base policy evaluations it
/// made on this thread.
#[cfg(any(test, feature = "test-utils"))]
pub fn count_validation_work<R>(f: impl FnOnce() -> R) -> (R, ValidationWork) {
    let before = WORK.get();
    let result = f();
    let after = WORK.get();
    let work = ValidationWork {
        registry_decodes: after.registry_decodes - before.registry_decodes,
        base_policy_evaluations: after.base_policy_evaluations - before.base_policy_evaluations,
    };
    (result, work)
}

/// Add work to this thread's [`count_validation_work`] totals.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn record_work(record: impl FnOnce(&mut ValidationWork)) {
    let mut work = WORK.get();
    record(&mut work);
    WORK.set(work);
}

#[cfg(test)]
mod tests;
