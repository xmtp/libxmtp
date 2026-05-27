use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions, MetadataPolicy as MetadataPolicyProto,
};

/// Builder for [`ComponentPermissions`].
///
/// Construct via the generated builder rather than positional arguments so
/// that the three same-typed `MetadataPolicyProto` arguments
/// (`insert`/`update`/`delete`) can't be accidentally swapped at the call
/// site:
///
/// ```ignore
/// let perms = component_permissions()
///     .insert(allow_policy)
///     .update(admin_only_policy)
///     .delete(super_admin_only_policy)
///     .call();
/// ```
#[bon::builder]
pub fn component_permissions(
    insert: MetadataPolicyProto,
    update: MetadataPolicyProto,
    delete: MetadataPolicyProto,
) -> ComponentPermissions {
    ComponentPermissions {
        insert_policy: Some(insert),
        update_policy: Some(update),
        delete_policy: Some(delete),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{
        Kind as MetadataPolicyKind, MetadataBasePolicy,
    };

    fn make_base_policy(base: MetadataBasePolicy) -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(base as i32)),
        }
    }

    fn allow() -> MetadataPolicyProto {
        make_base_policy(MetadataBasePolicy::Allow)
    }

    fn deny() -> MetadataPolicyProto {
        make_base_policy(MetadataBasePolicy::Deny)
    }

    fn admin_only() -> MetadataPolicyProto {
        make_base_policy(MetadataBasePolicy::AllowIfAdmin)
    }

    #[xmtp_common::test]
    fn test_permission_encode_decode_round_trip() {
        let perm = component_permissions()
            .insert(allow())
            .update(admin_only())
            .delete(deny())
            .call();
        let bytes = perm.encode_to_vec();
        let decoded = ComponentPermissions::decode(bytes.as_slice()).unwrap();
        assert_eq!(perm, decoded);
    }

    #[xmtp_common::test]
    fn test_all_deny() {
        let perm = component_permissions()
            .insert(deny())
            .update(deny())
            .delete(deny())
            .call();
        let bytes = perm.encode_to_vec();
        let decoded = ComponentPermissions::decode(bytes.as_slice()).unwrap();
        assert_eq!(perm, decoded);
    }

    #[xmtp_common::test]
    fn test_builder_sets_all_fields() {
        let perm = component_permissions()
            .insert(allow())
            .update(admin_only())
            .delete(deny())
            .call();
        assert!(perm.insert_policy.is_some());
        assert!(perm.update_policy.is_some());
        assert!(perm.delete_policy.is_some());
    }
}
