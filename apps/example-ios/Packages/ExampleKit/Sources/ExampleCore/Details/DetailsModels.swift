import Foundation

public enum GroupRole: String, Sendable {
	case member = "Member", admin = "Admin", superAdmin = "Super admin"
}

public struct GroupMember: Identifiable, Equatable, Sendable {
	public var id: String
	public var handle: String?
	public var otherHandles: [String]
	public var role: GroupRole
	public init(id: String, handle: String? = nil, otherHandles: [String] = [], role: GroupRole = .member) {
		self.id = id
		self.handle = handle
		self.otherHandles = otherHandles
		self.role = role
	}

	public var title: String {
		handle ?? String(id.prefix(12))
	}
}

public enum GroupPolicy: String, CaseIterable, Sendable {
	case allow = "All members", deny = "Denied", admin = "Admins", superAdmin = "Super admins", custom = "Custom"
}

public enum GroupPermissionKind: String, CaseIterable, Sendable {
	case addMember, removeMember, addAdmin, removeAdmin, updateMetadata
}

public enum GroupMetadataKind: String, CaseIterable, Sendable {
	case name, description, imageUrl, appData, disappearing
}

public struct PermissionWrite: Equatable, Sendable {
	public var kind: GroupPermissionKind
	public var policy: GroupPolicy
	public var field: GroupMetadataKind?
	public init(_ kind: GroupPermissionKind, _ policy: GroupPolicy, _ field: GroupMetadataKind? = nil) {
		self.kind = kind
		self.policy = policy
		self.field = field
	}

	public var label: String {
		field?.rawValue ?? kind.rawValue
	}
}

public enum PermissionPreset {
	public static func writes(adminOnly: Bool) -> [PermissionWrite] {
		let member: GroupPolicy = adminOnly ? .admin : .allow
		return [
			.init(.addMember, member),
			.init(.removeMember, .admin),
			.init(.addAdmin, .superAdmin),
			.init(.removeAdmin, .superAdmin),
		] + [GroupMetadataKind.name, .description, .imageUrl, .appData].map { .init(.updateMetadata, member, $0) } + [.init(
			.updateMetadata,
			.admin,
			.disappearing,
		)]
	}
}

public enum DisappearingRules {
	public static func nanoseconds(_ text: String) throws -> Int64? {
		guard let seconds = Int64(text), seconds >= 0 else { throw DetailsError.invalidDuration }
		let result = seconds.multipliedReportingOverflow(by: 1_000_000_000)
		guard !result.overflow else { throw DetailsError.invalidDuration }
		return seconds == 0 ? nil : result.partialValue
	}
}

public enum DetailsError: Error, Equatable {
	case unavailable, invalidDuration, failed
}

public struct ConversationDetails: Equatable, Sendable {
	public var record: ConversationRecord
	public var description: String
	public var members: [GroupMember]
	public var policies: [PermissionWrite]
	public var preset: String
	public var retentionNs: Int64
	public var pendingLeave: Bool
	public init(
		record: ConversationRecord,
		description: String = "",
		members: [GroupMember] = [],
		policies: [PermissionWrite] = [],
		preset: String = "Custom",
		retentionNs: Int64 = 0,
		pendingLeave: Bool = false,
	) {
		self.record = record
		self.description = description
		self.members = members
		self.policies = policies
		self.preset = preset
		self.retentionNs = retentionNs
		self.pendingLeave = pendingLeave
	}

	public func avatarLabel(alias: String? = nil) -> String {
		let title = record.title(alias: alias)
		if record.kind == .group {
			return DisplayInitials.group(title)
		}
		let hasAlias = !(alias?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
		let isInbox = !hasAlias && record.peerHandle?.isEmpty != false && record.peerInboxID != nil
		return DisplayInitials.person(title, isInboxID: isInbox)
	}

	public func canChangeRole(_ target: GroupMember, own: String) -> Bool {
		canManage(target, own: own, kind: target.role == .admin ? .removeAdmin : .addAdmin)
	}

	public func canRemoveMember(_ target: GroupMember, own: String) -> Bool {
		canManage(target, own: own, kind: .removeMember)
	}

	private func canManage(_ target: GroupMember, own: String, kind: GroupPermissionKind) -> Bool {
		guard record.kind == .group, target.id != own, target.role != .superAdmin,
		      members.contains(target), let actor = members.first(where: { $0.id == own }),
		      let policy = policies.first(where: { $0.kind == kind && $0.field == nil })?.policy else { return false }
		switch policy {
		case .allow: return true
		case .admin: return actor.role == .admin || actor.role == .superAdmin
		case .superAdmin: return actor.role == .superAdmin
		case .deny, .custom: return false
		}
	}

	public func canChangePermissionPreset(own: String) -> Bool {
		record.kind == .group && members.contains { $0.id == own && $0.role == .superAdmin }
	}

	public func canLeave(own: String) -> Bool {
		record.kind == .group && members.count > 1 && !members
			.contains { $0.id == own && $0.role == .superAdmin } && !pendingLeave
	}
}

public enum DetailsMutation: Sendable {
	case add(String), remove(String), admin(String, Bool), edit(String, String), leave, block, disappearing(Int64?)
}

@MainActor public protocol GroupQueries: AnyObject {
	func load() async throws -> ConversationDetails
	func resolve(_ input: String) async throws -> GroupMember
	func mutate(_ action: DetailsMutation) async throws
	func permission(_ write: PermissionWrite) async throws
}
