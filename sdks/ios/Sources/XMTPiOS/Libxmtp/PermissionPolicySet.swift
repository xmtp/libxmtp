import Foundation

public enum PermissionOption {
	case allow
	case deny
	case admin
	case superAdmin
	case unknown

	static func toFfiPermissionPolicy(option: PermissionOption)
		-> FfiPermissionPolicy
	{
		switch option {
		case .allow:
			.allow
		case .deny:
			.deny
		case .admin:
			.admin
		case .superAdmin:
			.superAdmin
		case .unknown:
			.other
		}
	}

	static func fromFfiPermissionPolicy(ffiPolicy: FfiPermissionPolicy)
		-> PermissionOption
	{
		switch ffiPolicy {
		case .allow:
			.allow
		case .deny:
			.deny
		case .admin:
			.admin
		case .superAdmin:
			.superAdmin
		case .doesNotExist, .other:
			.unknown
		}
	}
}

public enum GroupPermissionPreconfiguration {
	case allMembers
	case adminOnly

	static func toFfiGroupPermissionOptions(
		option: GroupPermissionPreconfiguration
	) -> FfiGroupPermissionsOptions {
		switch option {
		case .allMembers:
			.default
		case .adminOnly:
			.adminOnly
		}
	}
}

public class PermissionPolicySet {
	public var addMemberPolicy: PermissionOption
	public var removeMemberPolicy: PermissionOption
	public var addAdminPolicy: PermissionOption
	public var removeAdminPolicy: PermissionOption
	public var updateGroupNamePolicy: PermissionOption
	public var updateGroupDescriptionPolicy: PermissionOption
	public var updateGroupImagePolicy: PermissionOption
	public var updateMessageDisappearingPolicy: PermissionOption
	public var updateAppDataPolicy: PermissionOption

	public init(
		addMemberPolicy: PermissionOption, removeMemberPolicy: PermissionOption,
		addAdminPolicy: PermissionOption, removeAdminPolicy: PermissionOption,
		updateGroupNamePolicy: PermissionOption,
		updateGroupDescriptionPolicy: PermissionOption,
		updateGroupImagePolicy: PermissionOption,
		updateMessageDisappearingPolicy: PermissionOption,
		updateAppDataPolicy: PermissionOption
	) {
		self.addMemberPolicy = addMemberPolicy
		self.removeMemberPolicy = removeMemberPolicy
		self.addAdminPolicy = addAdminPolicy
		self.removeAdminPolicy = removeAdminPolicy
		self.updateGroupNamePolicy = updateGroupNamePolicy
		self.updateGroupDescriptionPolicy = updateGroupDescriptionPolicy
		self.updateGroupImagePolicy = updateGroupImagePolicy
		self.updateMessageDisappearingPolicy = updateMessageDisappearingPolicy
		self.updateAppDataPolicy = updateAppDataPolicy
	}

	static func toFfiPermissionPolicySet(
		_ permissionPolicySet: PermissionPolicySet
	) -> FfiPermissionPolicySet {
		FfiPermissionPolicySet(
			addMemberPolicy: PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.addMemberPolicy
			),
			removeMemberPolicy: PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.removeMemberPolicy
			),
			addAdminPolicy: PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.addAdminPolicy
			),
			removeAdminPolicy: PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.removeAdminPolicy
			),
			updateGroupNamePolicy: PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.updateGroupNamePolicy
			),
			updateGroupDescriptionPolicy:
			PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.updateGroupDescriptionPolicy
			),
			updateGroupImageUrlSquarePolicy:
			PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.updateGroupImagePolicy
			),
			updateMessageDisappearingPolicy:
			PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.updateMessageDisappearingPolicy
			),
			updateAppDataPolicy:
			PermissionOption.toFfiPermissionPolicy(
				option: permissionPolicySet.updateAppDataPolicy
			)
		)
	}

	static func fromFfiPermissionPolicySet(
		_ ffiPermissionPolicySet: FfiPermissionPolicySet
	) -> PermissionPolicySet {
		PermissionPolicySet(
			addMemberPolicy: PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet.addMemberPolicy
			),
			removeMemberPolicy: PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet.removeMemberPolicy
			),
			addAdminPolicy: PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet.addAdminPolicy
			),
			removeAdminPolicy: PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet.removeAdminPolicy
			),
			updateGroupNamePolicy: PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet.updateGroupNamePolicy
			),
			updateGroupDescriptionPolicy:
			PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet
					.updateGroupDescriptionPolicy
			),
			updateGroupImagePolicy: PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet
					.updateGroupImageUrlSquarePolicy
			),
			updateMessageDisappearingPolicy:
			PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet
					.updateMessageDisappearingPolicy
			),
			updateAppDataPolicy:
			PermissionOption.fromFfiPermissionPolicy(
				ffiPolicy: ffiPermissionPolicySet.updateAppDataPolicy
			)
		)
	}
}
