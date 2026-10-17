import ExampleCore
import Foundation
import XmtpSdk

func metadataShape(_ descriptor: MetadataFieldDescriptor) -> FieldShape {
	switch descriptor.componentType {
	case .string: return descriptor.isUserField ? .unsupported : .string
	case .bytes: return descriptor.isUserField ? .unsupported : .bytes
	case let .map(key, value):
		if key == .inboxId && descriptor.isUserField {
			return value == .string ? .userString : .userBytes
		}
		return key == .bytes && value == .bytes && !descriptor.isUserField ? .byteMap : .unsupported
	case let .set(key): return descriptor.isUserField ? .unsupported : key == .bytes ? .byteSet : .inboxSet
	case .unknown: return .unsupported
	}
}

func metadataPolicy(_ policy: MetadataPolicy) -> (String, Bool) {
	switch policy {
	case let .base(base):
		switch base {
		case .allow: return ("All members", true)
		case .deny: return ("Denied", true)
		case .allowIfAdmin: return ("Admins", true)
		case .allowIfSuperAdmin: return ("Super admins", true)
		case .allowIfSelfOrNonMember: return ("Own entry", true)
		case let .unknown(tag): return ("Unsupported policy \(tag)", false)
		}
	case let .and(children), let .any(children):
		let mapped = children.map(metadataPolicy)
		let separator: String = {
			if case .and = policy {
				return " and "
			}
			return " or "
		}()
		return ("(" + mapped.map(\.0).joined(separator: separator) + ")", !children.isEmpty && mapped.allSatisfy(\.1))
	}
}

func metadataAllows(_ policy: MetadataPolicy, role: GroupRole, ownEntry: Bool) -> Bool {
	switch policy {
	case let .base(base):
		switch base {
		case .allow: true
		case .deny, .unknown: false
		case .allowIfAdmin: role == .admin || role == .superAdmin
		case .allowIfSuperAdmin: role == .superAdmin
		case .allowIfSelfOrNonMember: ownEntry
		}
	case let .and(children): !children.isEmpty && children
		.allSatisfy { metadataAllows($0, role: role, ownEntry: ownEntry) }
	case let .any(children): children.contains { metadataAllows($0, role: role, ownEntry: ownEntry) }
	}
}

private func scalarText(_ value: FieldValue) -> String {
	switch value {
	case let .string(text): text
	case let .bytes(data): data.map { String(format: "%02x", $0) }.joined()
	}
}

private func keyText(_ value: FieldKey) -> String {
	switch value {
	case let .inboxId(text): text
	case let .bytes(data): data.map { String(format: "%02x", $0) }.joined()
	}
}

func metadataField(_ descriptor: MetadataFieldDescriptor, value: MetadataValue?,
                   role: GroupRole = .member) -> MetadataField
{
	let policies = [descriptor.permissions.insert, descriptor.permissions.update, descriptor.permissions.delete]
		.map(metadataPolicy)
	var result = MetadataField(
		id: descriptor.field.componentId,
		name: descriptor.field.name ?? "Component \(descriptor.field.componentId)",
		shape: metadataShape(descriptor),
		policy: zip(["Insert", "Update", "Delete"], policies).map { "\($0.0): \($0.1.0)" }.joined(separator: "; "),
		editable: policies.allSatisfy(\.1),
		present: value != nil,
		componentPresent: value != nil,
		userField: descriptor.isUserField,
		canInsert: metadataAllows(descriptor.permissions.insert, role: role, ownEntry: descriptor.isUserField),
		canUpdate: metadataAllows(descriptor.permissions.update, role: role, ownEntry: descriptor.isUserField),
		canDelete: metadataAllows(descriptor.permissions.delete, role: role, ownEntry: descriptor.isUserField),
	)
	switch value {
	case let .scalar(scalar): result.scalar = scalarText(scalar)
	case let .map(entries): result.entries = entries.map { FieldEntry(keyText($0.key), scalarText($0.value)) }
	case let .set(keys): result.entries = keys.map { FieldEntry(keyText($0)) }
	case nil: break
	}
	return result
}

@MainActor public final class SDKMetadataQueries: MetadataQueries {
	private let client: SDKClient
	private let id: String
	private let reads: AsyncLimiter
	private let valid: @MainActor () -> Bool
	public init(client: SDKClient, id: String, reads: AsyncLimiter, valid: @escaping @MainActor () -> Bool) {
		self.client = client
		self.id = id
		self.reads = reads
		self.valid = valid
	}

	private func conversation() async throws -> Conversation {
		guard let value = try await client.conversations.getById(id: id) else { throw DetailsError.unavailable }
		return value
	}

	private func check() throws {
		guard valid() else { throw CancellationError() }
	}

	private func ownRole(_ conversation: Conversation) async throws -> GroupRole {
		let members = try await reads.withPermit { try await conversation.members() }
		let own = members.first { $0.inboxId == client.inboxId() }
		return own?.permissionLevel == .superAdmin ? .superAdmin : own?.permissionLevel == .admin ? .admin : .member
	}

	public func load() async throws -> MetadataSnapshot {
		try check()
		let conversation = try await conversation()
		let descriptors = try await reads.withPermit { try await conversation.metadataFields() }
		let role = try await ownRole(conversation)
		var fields: [MetadataField] = []
		let users = descriptors.filter { $0.isUserField && metadataShape($0) != .unsupported }
		let profiles = try await reads
			.withPermit { try await conversation.userData(fields: users.map(\.field), inboxIds: nil) }
		for descriptor in descriptors
			where descriptor.isUserField || (0xC000 ... 0xFEFF).contains(descriptor.field.componentId)
		{
			var field = metadataField(descriptor, value: nil, role: role)
			if field.shape != .unsupported {
				if descriptor.isUserField {
					let value = profiles[client.inboxId()]?.first { $0.field.componentId == descriptor.field.componentId }?.value
					field.present = value != nil
					field.componentPresent = value != nil
					field.scalar = value.map(scalarText) ?? ""
					if field.immutable {
						field.componentPresent = try await reads
							.withPermit { try await conversation.metadataValue(field: descriptor.field) != nil }
					}
				} else {
					let value = try await reads.withPermit { try await conversation.metadataValue(field: descriptor.field) }
					field = metadataField(descriptor, value: value, role: role)
				}
			}
			fields.append(field)
		}
		let memberFields = profiles.filter { $0.key != client.inboxId() }.map { inbox, values in
			MemberMetadata(inboxID: inbox, fields: users.map { descriptor in
				var field = metadataField(descriptor, value: nil)
				let value = values.first { $0.field.componentId == descriptor.field.componentId }?.value
				field.present = value != nil
				field.scalar = value.map(scalarText) ?? ""
				field.editable = false
				return field
			})
		}
		let group: Bool = {
			if case .group = conversation {
				return true
			}
			return false
		}()
		let ids = Set(descriptors.map(\.field.componentId))
		let missing = client.serverConfiguration().applicationComponents
			.filter { (group ? $0.inGroups : $0.inDms) && !ids.contains($0.componentId) }.map(\.name)
		try check()
		return MetadataSnapshot(fields: fields.sorted { $0.id < $1.id }, members: memberFields, missing: missing)
	}

	private func value(_ text: String, shape: FieldShape) throws -> FieldValue {
		let canonical = try MetadataRules.scalar(text, shape: shape)
		return try shape == .string || shape == .userString ? .string(canonical) : .bytes(MetadataRules.bytes(canonical))
	}

	private func key(_ text: String, shape: FieldShape) throws -> FieldKey {
		let canonical = try MetadataRules.key(text, shape: shape)
		return try shape == .inboxSet ? .inboxId(canonical) : .bytes(MetadataRules.bytes(canonical))
	}

	public func edit(_ edit: MetadataEdit, baseline: MetadataSnapshot) async throws {
		try check()
		let current = try await load()
		let conversation = try await conversation()
		let descriptors = try await reads.withPermit { try await conversation.metadataFields() }
		func field(_ id: UInt16) throws -> (MetadataField, MetadataFieldRef) {
			guard let old = baseline.fields.first(where: { $0.id == id }), let now = current.fields.first(
				where: { $0.id == id },
			),
				let descriptor = descriptors.first(where: { $0.field.componentId == id })
			else { throw MetadataRuleError.changed }
			try MetadataRules.unchanged(old, now)
			let latest = metadataField(descriptor, value: nil)
			guard latest.shape == now.shape, latest.policy == now.policy, latest.editable == now.editable,
			      latest.userField == now.userField else { throw MetadataRuleError.changed }
			return (now, descriptor.field)
		}
		switch edit {
		case let .scalar(id, text): try await editScalar(text, field: field(id), conversation: conversation)
		case let .entry(id, action, text, content):
			try await editEntry(action, text: text, content: content, field: field(id), conversation: conversation)
		case let .own(values): try await editOwn(values, field: field, conversation: conversation)
		}
	}

	private func editScalar(
		_ text: String?,
		field pair: (MetadataField, MetadataFieldRef),
		conversation: Conversation,
	) async throws {
		let (field, ref) = pair
		guard !field.userField && (field.shape == .string || field.shape == .bytes)
		else { throw MetadataRuleError.unsupported }
		guard !field.immutable || text != nil else { throw MetadataRuleError.immutable }
		guard text == nil ? field.allows(.delete) : field.canSet else { throw MetadataRuleError.immutable }
		let operation: ComponentMutation = try text.map { try .replace(value($0, shape: field.shape)) } ?? .remove
		try check()
		try await conversation.updateMetadataField(field: ref, operation: operation)
	}

	private func editEntry(
		_ action: FieldEntryAction,
		text: String,
		content: String,
		field pair: (MetadataField, MetadataFieldRef),
		conversation: Conversation,
	) async throws {
		let (field, ref) = pair
		guard field.allows(action) else { throw MetadataRuleError.immutable }
		guard !field.userField else { throw MetadataRuleError.unsupported }
		guard !field.immutable || action == .insert else { throw MetadataRuleError.immutable }
		let canonical = try MetadataRules.key(text, shape: field.shape)
		let exists = field.entries.contains { $0.key == canonical }
		guard action == .insert ? !exists : exists else { throw MetadataRuleError.entryChanged }
		var next = field.entries.filter { $0.key != canonical }
		let scalar = field.shape == .byteMap && action != .delete ? try MetadataRules.scalar(content, shape: field.shape) : ""
		if action != .delete {
			next.append(FieldEntry(canonical, scalar))
		}
		try MetadataRules.collection(next, shape: field.shape)
		let key = try key(canonical, shape: field.shape)
		let operation: ComponentMutation
		if field.shape == .byteMap {
			switch action {
			case .insert: operation = try .mapDelta([.insert(key, value(content, shape: .byteMap))])
			case .update: operation = try .mapDelta([.update(key, value(content, shape: .byteMap))])
			case .delete: operation = .mapDelta([.delete(key)])
			}
		} else {
			guard action != .update else { throw MetadataRuleError.unsupported }
			operation = .setDelta([action == .insert ? .insert(key) : .delete(key)])
		}
		try check()
		try await conversation.updateMetadataField(field: ref, operation: operation)
	}

	private func editOwn(
		_ values: [UInt16: String?],
		field: (UInt16) throws -> (MetadataField, MetadataFieldRef),
		conversation: Conversation,
	) async throws {
		var updates: [UserFieldUpdate] = []
		for (id, text) in values {
			let (field, ref) = try field(id)
			guard field.userField else { throw MetadataRuleError.unsupported }
			let value = try text.map { try self.value($0, shape: field.shape) }
			if field.present == (value != nil) && field.scalar == (value.map(scalarText) ?? "") {
				continue
			}
			guard value == nil ? field.allows(.delete) : field.canSet else { throw MetadataRuleError.immutable }
			let snapshot = try await reads.withPermit { try await conversation.metadataValue(field: ref) }
			guard !field.immutable || (snapshot == nil && value != nil) else { throw MetadataRuleError.immutable }
			var entries: [FieldEntry] = []
			if case let .map(stored) = snapshot {
				entries = stored.filter { keyText($0.key) != client.inboxId() }.map { FieldEntry(
					keyText($0.key),
					scalarText($0.value),
				) }
			}
			if let value {
				entries.append(FieldEntry(client.inboxId(), scalarText(value)))
			}
			try MetadataRules.collection(entries, shape: field.shape)
			updates.append(UserFieldUpdate(field: ref, value: value))
		}
		if !updates.isEmpty {
			try check()
			try await conversation.updateUserData(values: updates)
		}
	}
}
