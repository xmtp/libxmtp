//! The leaves of `groups/tests/test_proposals_permissions.rs` that need the client.

use crate::tester;

/// The cases run in a loop instead of `#[rstest]` `#[case]` attributes on
/// purpose. This test is compiled into the wasm32 test binary, and the wasm
/// derivation sets `RSTEST_TIMEOUT`. With that variable set, `rstest` routes a
/// *sync* case through `execute_with_timeout_sync`, which calls
/// `std::thread::spawn` — unsupported on wasm32, so the binary aborts.
///
/// This test is async, so it would take rstest's `execute_with_timeout_async`
/// path (futures-timer, no thread) and would not abort today. The loop keeps it
/// that way regardless: making this fn sync later must not be able to wedge the
/// wasm binary. Prefer a loop over `#[rstest]` in wasm-visible tests.
///
/// Each iteration builds its own client so one case's groups and permission
/// commits cannot leak into the next.
#[xmtp_common::test(unwrap_try = true)]
async fn test_dictionary_native_permissions_presets() {
    use crate::groups::{
        group_permissions::PreconfiguredPolicies, intents::PermissionPolicyOption,
        intents::PermissionUpdateType,
    };

    let cases: [(&str, PreconfiguredPolicies, bool, PermissionPolicyOption); 3] = [
        (
            "default",
            PreconfiguredPolicies::Default,
            false,
            PermissionPolicyOption::Allow,
        ),
        (
            "admins_only",
            PreconfiguredPolicies::AdminsOnly,
            false,
            PermissionPolicyOption::AdminOnly,
        ),
        (
            "custom_admins",
            PreconfiguredPolicies::Default,
            true,
            PermissionPolicyOption::Allow,
        ),
    ];

    for (case_name, preset, custom_admins, add_member_policy) in cases {
        tester!(alix);
        let mut expected = preset.to_policy_set();
        if custom_admins {
            use crate::groups::group_permissions::PermissionsPolicies;
            expected.add_admin_policy = PermissionsPolicies::allow_if_actor_admin();
            expected.remove_admin_policy = PermissionsPolicies::allow_if_actor_admin();
        }
        let group = alix.create_group(Some(expected.clone()), None).unwrap();
        assert_eq!(
            group.permissions().unwrap().policies,
            expected,
            "case {case_name}: created group did not match the requested policy set"
        );

        group
            .update_permission_policy(
                PermissionUpdateType::AddMember,
                PermissionPolicyOption::Deny,
                None,
            )
            .await
            .unwrap();
        assert!(
            PreconfiguredPolicies::from_policy_set(&group.permissions().unwrap().policies).is_err(),
            "case {case_name}: a denied add_member policy should not map back to a preset"
        );
        // `PermissionPolicyOption` is not `Copy`, and the call below consumes
        // it, so capture the label for the assertion message first.
        let policy_label = format!("{add_member_policy:?}");
        group
            .update_permission_policy(PermissionUpdateType::AddMember, add_member_policy, None)
            .await
            .unwrap();
        let actual = group.permissions().unwrap().policies;
        assert_eq!(
            actual, expected,
            "case {case_name}: restoring {policy_label} did not return the original policy set"
        );
        if custom_admins {
            assert!(
                PreconfiguredPolicies::from_policy_set(&actual).is_err(),
                "case {case_name}: custom admin policies should not map back to a preset"
            );
        } else {
            assert_eq!(
                PreconfiguredPolicies::from_policy_set(&actual).unwrap(),
                preset,
                "case {case_name}: restored policy set did not map back to {preset:?}"
            );
        }
    }
}
