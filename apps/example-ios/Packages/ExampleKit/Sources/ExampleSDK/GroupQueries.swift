import ExampleCore
import Foundation
import XmtpSdk

struct SDKPermissionWrite {
	let kind: PermissionUpdateKind
	let policy: PermissionPolicy
	let field: MetadataFieldKind?
}

func sdkPermission(_ write: PermissionWrite) -> SDKPermissionWrite {
	SDKPermissionWrite(
		kind: sdkPermissionKind(write.kind),
		policy: sdkPermissionPolicy(write.policy),
		field: write.field.map(sdkMetadataKind),
	)
}

private func sdkPermissionKind(_ kind: GroupPermissionKind) -> PermissionUpdateKind {
	switch kind {
	case .addMember: .addMember
	case .removeMember: .removeMember
	case .addAdmin: .addAdmin
	case .removeAdmin: .removeAdmin
	case .updateMetadata: .updateMetadata
	}
}

private func sdkPermissionPolicy(_ policy: GroupPolicy) -> PermissionPolicy {
	switch policy {
	case .allow: .allow
	case .deny: .deny
	case .admin: .admin
	case .superAdmin: .superAdmin
	case .custom: .other
	}
}

private func sdkMetadataKind(_ field: GroupMetadataKind) -> MetadataFieldKind {
	switch field {
	case .name: .name
	case .description: .description
	case .imageUrl: .imageUrl
	case .appData: .appData
	case .disappearing: .disappearing
	}
}

func corePolicy(_ policy: PermissionPolicy) -> GroupPolicy {
	switch policy {
	case .allow: .allow
	case .deny: .deny
	case .admin: .admin
	case .superAdmin: .superAdmin
	case .doesNotExist, .other: .custom
	}
}

func corePreset(_ type: GroupPolicyType) -> String {
	switch type {
	case .allMembers: "All members"
	case .adminOnly: "Admins only"
	case .custom: "Custom"
	}
}

@MainActor public final class SDKGroupQueries: GroupQueries {
	private let client: SDKClient
	private let id: String
	private let reads: AsyncLimiter
	private let valid: @MainActor () -> Bool
	private let privacyChanged: @MainActor (Bool) -> Void
	public init(
		client: SDKClient,
		id: String,
		reads: AsyncLimiter,
		valid: @escaping @MainActor () -> Bool,
		privacyChanged: @escaping @MainActor (Bool) -> Void = { _ in },
	) {
		self.client = client
		self.id = id
		self.reads = reads
		self.valid = valid
		self.privacyChanged = privacyChanged
	}

	private func check() throws {
		guard valid() else { throw CancellationError() }
	}

	private func conversation() async throws -> Conversation {
		guard let value = try await client.conversations.getById(id: id) else { throw DetailsError.unavailable }
		return value
	}

	public func load() async throws -> ConversationDetails {
		let value = try await conversation()
		try await value.sync()
		try check()
		let members = try await reads.withPermit { try await value.members() }.map { member in GroupMember(
			id: member.inboxId,
			handle: member.handle,
			otherHandles: member.otherHandles,
			role: member.permissionLevel == .superAdmin ? .superAdmin : member.permissionLevel == .admin ? .admin : .member,
		) }
		switch value {
		case let .group(group):
			let state = try await reads.withPermit { try await group.state() }
			let policy = state.permissions.policySet
			let policies: [PermissionWrite] = [
				.init(.addMember, corePolicy(policy.addMember)),
				.init(.removeMember, corePolicy(policy.removeMember)),
				.init(.addAdmin, corePolicy(policy.addAdmin)),
				.init(.removeAdmin, corePolicy(policy.removeAdmin)),
				.init(.updateMetadata, corePolicy(policy.updateName), .name),
				.init(.updateMetadata, corePolicy(policy.updateDescription), .description),
				.init(.updateMetadata, corePolicy(policy.updateImage), .imageUrl),
				.init(.updateMetadata, corePolicy(policy.updateAppData), .appData),
				.init(.updateMetadata, corePolicy(policy.updateDisappearing), .disappearing),
			]
			return ConversationDetails(
				record: ConversationRecord(id: id, kind: .group, name: state.name),
				description: state.description,
				members: members,
				policies: policies,
				preset: corePreset(state.permissions.policyType),
				retentionNs: state.common.disappearingSettings?.retentionNs ?? 0,
				pendingLeave: state.membershipState == .pendingRemove,
			)
		case let .dm(dm):
			let state = try await reads.withPermit { try await dm.state() }
			let peer = members.first { $0.id != client.inboxId() }
			return ConversationDetails(
				record: ConversationRecord(id: id, kind: .dm, peerInboxID: peer?.id, peerHandle: peer?.handle),
				members: members,
				retentionNs: state.disappearingSettings?.retentionNs ?? 0,
			)
		}
	}

	public func resolve(_ input: String) async throws -> GroupMember {
		let identity = SDKIdentityQueries(client: client, limiter: reads)
		let id = try await IdentityInput.parse(input).resolve(using: identity)
		var handle: String?
		do { handle = try await identity.handles(inboxIDs: [id]).first?.handle } catch { handle = nil }
		return GroupMember(id: id, handle: handle)
	}

	public func permission(_ write: PermissionWrite) async throws {
		try check()
		privacyChanged(true)
		defer { privacyChanged(false) }
		guard case let .group(group) = try await conversation() else { throw DetailsError.unavailable }
		try check()
		let mapped = sdkPermission(write)
		try await group.updatePermission(kind: mapped.kind, policy: mapped.policy, metadataField: mapped.field)
		try check()
	}

	public func mutate(_ action: DetailsMutation) async throws {
		try check()
		privacyChanged(true)
		defer { privacyChanged(false) }
		let value = try await conversation()
		try check()
		switch action {
		case .block: try await value.updateConsentState(state: .denied)
		case let .disappearing(nanoseconds): try await value
			.updateDisappearingSettings(settings: nanoseconds.map { DisappearingSettings(
				from: Timestamp(ns: Int64(Date().timeIntervalSince1970 * 1_000_000_000)),
				retentionNs: $0,
			) })
		default:
			guard case let .group(group) = value else { throw DetailsError.unavailable }
			try await mutateGroup(group, action: action)
		}
	}

	private func mutateGroup(_ group: XmtpSdk.Group, action: DetailsMutation) async throws {
		switch action {
		case let .add(id): _ = try await group.addMembers(members: [id])
		case let .remove(id): try await group.removeMembers(members: [id])
		case let .admin(id, enabled): try await setAdmin(group, id: id, enabled: enabled)
		case let .edit(name, description):
			try await group.updateName(value: name)
			try check()
			try await group.updateDescription(value: description)
		case .leave: try await group.requestRemoval()
		default: break
		}
	}

	private func setAdmin(_ group: XmtpSdk.Group, id: String, enabled: Bool) async throws {
		if enabled {
			try await group.addAdmin(inboxId: id)
		} else {
			try await group.removeAdmin(inboxId: id)
		}
	}
}
