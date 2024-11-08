import Foundation
import LibXMTP

public enum ConsentState: String, Codable {
	case allowed, denied, unknown
}
public enum EntryType: String, Codable {
	case address, conversation_id, inbox_id
}

public struct ConsentListEntry: Codable, Hashable {
	static func address(_ address: String, type: ConsentState = .unknown)
		-> ConsentListEntry
	{
		ConsentListEntry(value: address, entryType: .address, consentType: type)
	}

	static func conversationId(
		conversationId: String, type: ConsentState = ConsentState.unknown
	) -> ConsentListEntry {
		ConsentListEntry(
			value: conversationId, entryType: .conversation_id, consentType: type)
	}

	static func inboxId(_ inboxId: String, type: ConsentState = .unknown)
		-> ConsentListEntry
	{
		ConsentListEntry(
			value: inboxId, entryType: .inbox_id, consentType: type)
	}

	public var value: String
	public var entryType: EntryType
	public var consentType: ConsentState

	var key: String {
		"\(entryType)-\(value)"
	}
}

public enum ContactError: Error {
	case invalidIdentifier
}

public actor EntriesManager {
	public var map: [String: ConsentListEntry] = [:]

	func set(_ key: String, _ object: ConsentListEntry) {
		map[key] = object
	}

	func get(_ key: String) -> ConsentListEntry? {
		map[key]
	}
}

public class ConsentList {
	public let entriesManager = EntriesManager()
	var lastFetched: Date?
	var client: Client
	var ffiClient: FfiXmtpClient

	init(client: Client, ffiClient: FfiXmtpClient) {
		self.client = client
		self.ffiClient = ffiClient
	}

	func setConsentState(entries: [ConsentListEntry]) async throws {
		try await ffiClient.setConsentStates(records: entries.map(\.toFFI))
	}

	func addressState(address: String) async throws -> ConsentState {
		return try await ffiClient.getConsentState(
			entityType: .address,
			entity: address
		).fromFFI
	}

	func conversationState(conversationId: String) async throws -> ConsentState {
		return try await ffiClient.getConsentState(
			entityType: .conversationId,
			entity: conversationId
		).fromFFI
	}

	func inboxIdState(inboxId: String) async throws -> ConsentState {
		return try await ffiClient.getConsentState(
			entityType: .inboxId,
			entity: inboxId
		).fromFFI
	}
}

/// Provides access to contact bundles.
public actor PrivatePreferences {
	var client: Client
	var ffiClient: FfiXmtpClient
	public var consentList: ConsentList

	init(client: Client, ffiClient: FfiXmtpClient) {
		self.client = client
		self.ffiClient = ffiClient
		consentList = ConsentList(client: client, ffiClient: ffiClient)
	}
}
