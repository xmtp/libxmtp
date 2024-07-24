import Foundation
import LibXMTP

public enum PermissionOption {
    case allow
    case deny
    case admin
    case superAdmin
    case unknown

    static func toFfiPermissionPolicy(option: PermissionOption) -> FfiPermissionPolicy {
        switch option {
        case .allow:
            return .allow
        case .deny:
            return .deny
        case .admin:
            return .admin
        case .superAdmin:
            return .superAdmin
        case .unknown:
            return .other
        }
    }

    static func fromFfiPermissionPolicy(ffiPolicy: FfiPermissionPolicy) -> PermissionOption {
        switch ffiPolicy {
        case .allow:
            return .allow
        case .deny:
            return .deny
        case .admin:
            return .admin
        case .superAdmin:
            return .superAdmin
        case .doesNotExist, .other:
            return .unknown
        }
    }
}

public enum GroupPermissionPreconfiguration {
    case allMembers
    case adminOnly

    static func toFfiGroupPermissionOptions(option: GroupPermissionPreconfiguration) -> FfiGroupPermissionsOptions {
        switch option {
        case .allMembers:
            return .allMembers
        case .adminOnly:
            return .adminOnly
        }
    }
}

public class PermissionPolicySet {
    var addMemberPolicy: PermissionOption
    var removeMemberPolicy: PermissionOption
    var addAdminPolicy: PermissionOption
    var removeAdminPolicy: PermissionOption
    var updateGroupNamePolicy: PermissionOption
    var updateGroupDescriptionPolicy: PermissionOption
    var updateGroupImagePolicy: PermissionOption
    var updateGroupPinnedFrameUrlPolicy: PermissionOption

    init(addMemberPolicy: PermissionOption, removeMemberPolicy: PermissionOption, addAdminPolicy: PermissionOption, removeAdminPolicy: PermissionOption, updateGroupNamePolicy: PermissionOption, updateGroupDescriptionPolicy: PermissionOption, updateGroupImagePolicy: PermissionOption, updateGroupPinnedFrameUrlPolicy: PermissionOption) {
        self.addMemberPolicy = addMemberPolicy
        self.removeMemberPolicy = removeMemberPolicy
        self.addAdminPolicy = addAdminPolicy
        self.removeAdminPolicy = removeAdminPolicy
        self.updateGroupNamePolicy = updateGroupNamePolicy
        self.updateGroupDescriptionPolicy = updateGroupDescriptionPolicy
        self.updateGroupImagePolicy = updateGroupImagePolicy
        self.updateGroupPinnedFrameUrlPolicy = updateGroupPinnedFrameUrlPolicy
    }

    static func toFfiPermissionPolicySet(_ permissionPolicySet: PermissionPolicySet) -> FfiPermissionPolicySet {
        return FfiPermissionPolicySet(
            addMemberPolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.addMemberPolicy),
            removeMemberPolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.removeMemberPolicy),
            addAdminPolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.addAdminPolicy),
            removeAdminPolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.removeAdminPolicy),
            updateGroupNamePolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.updateGroupNamePolicy),
            updateGroupDescriptionPolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.updateGroupDescriptionPolicy),
            updateGroupImageUrlSquarePolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.updateGroupImagePolicy),
            updateGroupPinnedFrameUrlPolicy: PermissionOption.toFfiPermissionPolicy(option: permissionPolicySet.updateGroupPinnedFrameUrlPolicy)
        )
    }

    static func fromFfiPermissionPolicySet(_ ffiPermissionPolicySet: FfiPermissionPolicySet) -> PermissionPolicySet {
        return PermissionPolicySet(
            addMemberPolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.addMemberPolicy),
            removeMemberPolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.removeMemberPolicy),
            addAdminPolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.addAdminPolicy),
            removeAdminPolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.removeAdminPolicy),
            updateGroupNamePolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.updateGroupNamePolicy),
            updateGroupDescriptionPolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.updateGroupDescriptionPolicy),
            updateGroupImagePolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.updateGroupImageUrlSquarePolicy),
            updateGroupPinnedFrameUrlPolicy: PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.updateGroupPinnedFrameUrlPolicy)
        )
    }
}

