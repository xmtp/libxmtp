import Foundation

public enum FieldShape: String, CaseIterable, Sendable {
	case string, bytes, byteMap, byteSet, inboxSet, userString, userBytes, unsupported
}

public struct FieldEntry: Equatable, Sendable { public var key: String
	public var value: String
	public init(_ key: String, _ value: String = "") {
		self.key = key
		self.value = value
	}
}

public struct MetadataField: Identifiable, Equatable, Sendable {
	public var userField: Bool
	public var id: UInt16
	public var name: String
	public var shape: FieldShape
	public var policy: String
	public var editable: Bool
	public var present: Bool
	public var componentPresent: Bool
	public var scalar: String
	public var entries: [FieldEntry]
	public init(
		id: UInt16,
		name: String,
		shape: FieldShape,
		policy: String = "",
		editable: Bool = true,
		present: Bool = false,
		componentPresent: Bool = false,
		scalar: String = "",
		entries: [FieldEntry] = [],
		userField: Bool? = nil,
	) {
		self.userField = userField ?? (shape == .userString || shape == .userBytes)
		self.id = id
		self.name = name
		self.shape = shape
		self.policy = policy
		self.editable = editable
		self.present = present
		self.componentPresent = componentPresent
		self.scalar = scalar
		self.entries = entries
	}

	public var immutable: Bool {
		(0xFD00 ... 0xFEFF).contains(id)
	}

	public var canWrite: Bool {
		editable && shape != .unsupported && (!immutable || !componentPresent)
	}
}

public struct MemberMetadata: Equatable, Sendable { public var inboxID: String
	public var fields: [MetadataField]
	public init(inboxID: String, fields: [MetadataField]) {
		self.inboxID = inboxID
		self.fields = fields
	}
}

public struct MetadataSnapshot: Equatable, Sendable {
	public var fields: [MetadataField]
	public var members: [MemberMetadata]
	public var missing: [String]
	public init(fields: [MetadataField] = [], members: [MemberMetadata] = [], missing: [String] = []) {
		self.fields = fields
		self.members = members
		self.missing = missing
	}
}

public enum MetadataRuleError: Error, Equatable {
	case hex, inbox, valueLimit, collectionLimit, unsupported, changed, immutable, entryChanged
}

public enum MetadataRules {
	public static func bytes(_ text: String) throws -> Data {
		guard text.count.isMultiple(of: 2),
		      text.utf8.allSatisfy({ (48 ... 57).contains($0) || (65 ... 70).contains($0) || (97 ... 102).contains($0) })
		else { throw MetadataRuleError.hex }
		guard text.count <= 16384 else { throw MetadataRuleError.valueLimit }
		var result = Data()
		var index = text.startIndex
		while index < text.endIndex {
			let end = text.index(index, offsetBy: 2)
			guard let byte = UInt8(text[index ..< end], radix: 16) else { throw MetadataRuleError.hex }
			result.append(byte)
			index = end
		}
		return result
	}

	public static func inbox(_ text: String) throws -> String {
		guard text.utf8.count == 64,
		      text.utf8.allSatisfy({ (48 ... 57).contains($0) || (97 ... 102).contains($0) })
		else { throw MetadataRuleError.inbox }
		return text
	}

	public static func scalar(_ text: String, shape: FieldShape) throws -> String {
		switch shape {
		case .string, .userString: guard text.utf8.count <= 8192 else { throw MetadataRuleError.valueLimit }
			return text
		case .bytes, .byteMap, .userBytes: return try bytes(text).map { String(format: "%02x", $0) }.joined()
		default: throw MetadataRuleError.unsupported
		}
	}

	public static func key(_ text: String, shape: FieldShape) throws -> String {
		switch shape {
		case .byteMap, .byteSet: return try scalar(text, shape: .bytes)
		case .inboxSet, .userString, .userBytes: return try inbox(text)
		default: throw MetadataRuleError.unsupported
		}
	}

	private static func prefix(_ count: Int) -> Int {
		count < 64 ? 1 : count < 16384 ? 2 : 4
	}

	public static func collection(_ entries: [FieldEntry], shape: FieldShape) throws {
		var payload = 0
		for entry in entries {
			if shape == .inboxSet || shape == .userString || shape == .userBytes {
				_ = try inbox(entry.key)
				payload += 33
			} else {
				let count = try bytes(entry.key).count
				payload += count + prefix(count)
			}
			if shape == .byteMap || shape == .userBytes {
				let count = try bytes(entry.value).count
				payload += count + prefix(count)
			}
			if shape == .userString {
				let count = entry.value.utf8.count
				guard count <= 8192 else { throw MetadataRuleError.valueLimit }
				payload += count + prefix(count)
			}
			guard payload <= 65536 else { throw MetadataRuleError.collectionLimit }
		}
		guard payload + prefix(payload) <= 65536 else { throw MetadataRuleError.collectionLimit }
	}

	public static func unchanged(_ before: MetadataField, _ now: MetadataField) throws {
		guard before.id == now.id, before.shape == now.shape, before.policy == now.policy, before.userField == now.userField,
		      before.editable == now.editable else { throw MetadataRuleError.changed }
		guard now.canWrite else { throw MetadataRuleError.immutable }
	}
}

public enum FieldEntryAction: Sendable {
	case insert, update, delete
}

public enum MetadataEdit: Sendable {
	case scalar(UInt16, String?), entry(UInt16, FieldEntryAction, String, String), own([UInt16: String?])
}

@MainActor public protocol MetadataQueries: AnyObject {
	func load() async throws -> MetadataSnapshot
	func edit(_ edit: MetadataEdit, baseline: MetadataSnapshot) async throws
}
