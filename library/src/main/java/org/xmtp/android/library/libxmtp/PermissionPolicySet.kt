package uniffi.xmtpv3.org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiGroupPermissionsOptions
import uniffi.xmtpv3.FfiPermissionPolicy
import uniffi.xmtpv3.FfiPermissionPolicySet
enum class PermissionOption {
    Allow,
    Deny,
    Admin,
    SuperAdmin,
    Unknown;
    companion object {
        fun toFfiPermissionPolicy(option: PermissionOption): FfiPermissionPolicy {
            return when (option) {
                Allow -> FfiPermissionPolicy.ALLOW
                Deny -> FfiPermissionPolicy.DENY
                Admin -> FfiPermissionPolicy.ADMIN
                SuperAdmin -> FfiPermissionPolicy.SUPER_ADMIN
                Unknown -> FfiPermissionPolicy.OTHER
            }
        }
        fun fromFfiPermissionPolicy(ffiPolicy: FfiPermissionPolicy): PermissionOption {
            return when (ffiPolicy) {
                FfiPermissionPolicy.ALLOW -> Allow
                FfiPermissionPolicy.DENY -> Deny
                FfiPermissionPolicy.ADMIN -> Admin
                FfiPermissionPolicy.SUPER_ADMIN -> SuperAdmin
                FfiPermissionPolicy.DOES_NOT_EXIST -> Unknown
                FfiPermissionPolicy.OTHER -> Unknown
            }
        }
    }
}

enum class GroupPermissionPreconfiguration {
    ALL_MEMBERS,
    ADMIN_ONLY;

    companion object {
        fun toFfiGroupPermissionOptions(option: GroupPermissionPreconfiguration): FfiGroupPermissionsOptions {
            return when (option) {
                ALL_MEMBERS -> FfiGroupPermissionsOptions.ALL_MEMBERS
                ADMIN_ONLY -> FfiGroupPermissionsOptions.ADMIN_ONLY
            }
        }
    }
}

data class PermissionPolicySet(
    val addMemberPolicy: PermissionOption,
    val removeMemberPolicy: PermissionOption,
    val addAdminPolicy: PermissionOption,
    val removeAdminPolicy: PermissionOption,
    val updateGroupNamePolicy: PermissionOption,
    val updateGroupDescriptionPolicy: PermissionOption,
    val updateGroupImagePolicy: PermissionOption,
    val updateGroupPinnedFrameUrlPolicy: PermissionOption,
) {
    companion object {
        fun toFfiPermissionPolicySet(permissionPolicySet: PermissionPolicySet): FfiPermissionPolicySet {
            return FfiPermissionPolicySet(
                addMemberPolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.addMemberPolicy),
                removeMemberPolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.removeMemberPolicy),
                addAdminPolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.addAdminPolicy),
                removeAdminPolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.removeAdminPolicy),
                updateGroupNamePolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.updateGroupNamePolicy),
                updateGroupDescriptionPolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.updateGroupDescriptionPolicy),
                updateGroupImageUrlSquarePolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.updateGroupImagePolicy),
                updateGroupPinnedFrameUrlPolicy = PermissionOption.toFfiPermissionPolicy(permissionPolicySet.updateGroupPinnedFrameUrlPolicy)
            )
        }

        fun fromFfiPermissionPolicySet(ffiPermissionPolicySet: FfiPermissionPolicySet): PermissionPolicySet {
            return PermissionPolicySet(
                addMemberPolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.addMemberPolicy),
                removeMemberPolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.removeMemberPolicy),
                addAdminPolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.addAdminPolicy),
                removeAdminPolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.removeAdminPolicy),
                updateGroupNamePolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.updateGroupNamePolicy),
                updateGroupDescriptionPolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.updateGroupDescriptionPolicy),
                updateGroupImagePolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.updateGroupImageUrlSquarePolicy),
                updateGroupPinnedFrameUrlPolicy = PermissionOption.fromFfiPermissionPolicy(ffiPermissionPolicySet.updateGroupPinnedFrameUrlPolicy),
            )
        }
    }
}
