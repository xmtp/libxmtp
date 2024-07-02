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
    let ffiPermissionPolicySet: FfiPermissionPolicySet
    
    init(ffiPermissionPolicySet: FfiPermissionPolicySet) {
        self.ffiPermissionPolicySet = ffiPermissionPolicySet
    }
    
    var addMemberPolicy: PermissionOption {
        return PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.addMemberPolicy)
    }
    
    var removeMemberPolicy: PermissionOption {
        return PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.removeMemberPolicy)
    }
    
    var addAdminPolicy: PermissionOption {
        return PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.addAdminPolicy)
    }
    
    var removeAdminPolicy: PermissionOption {
        return PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.removeAdminPolicy)
    }
    
    var updateGroupNamePolicy: PermissionOption {
        return PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.updateGroupNamePolicy)
    }
    
    var updateGroupDescriptionPolicy: PermissionOption {
        return PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.updateGroupDescriptionPolicy)
    }
    
    var updateGroupImagePolicy: PermissionOption {
        return PermissionOption.fromFfiPermissionPolicy(ffiPolicy: ffiPermissionPolicySet.updateGroupImageUrlSquarePolicy)
    }
}
