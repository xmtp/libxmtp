import Foundation

public struct BackendProfile: Codable, Equatable, Sendable {
	public var id: UUID
	public var url: String
	// Codable omits labels because they live in encrypted preferences; synthesis needs this default.
	// swiftlint:disable:next implicit_optional_initialization
	public var name: String? = nil
	private enum CodingKeys: String, CodingKey { case id, url, inboxID, identity }
	public var inboxID: String?
	public var identity: String?
	public init(id: UUID = UUID(), url: String, inboxID: String? = nil, identity: String? = nil) {
		self.id = id; self.url = url; self.inboxID = inboxID; self.identity = identity
	}
}

public enum ResetPhase: String, Codable,
	Sendable { case stopping = "STOPPING", databaseRemoved = "DATABASE_REMOVED", filesRemoved = "FILES_REMOVED" }
public struct ResetJournal: Codable, Equatable,
	Sendable
{ public var profileID: UUID; public var phase: ResetPhase; public init(
	profileID: UUID,
	phase: ResetPhase,
) {
	self.profileID = profileID; self.phase = phase
} }
public struct SessionIndex: Codable, Equatable, Sendable {
	public var profiles: [BackendProfile] = []
	public var activeID: UUID?
	public var signedIn = false
	public var reset: ResetJournal?
	public var appearance = "system"
	public init() {}
}

public struct ProfilePaths: Sendable {
	public let root: URL
	public var database: URL {
		root.appendingPathComponent("sdk/messages.db3")
	}

	public var attachments: URL {
		root.appendingPathComponent("sdk/attachments", isDirectory: true)
	}

	public var temp: URL {
		root.appendingPathComponent("temp", isDirectory: true)
	}

	public var preferences: URL {
		root.appendingPathComponent("prefs", isDirectory: true)
	}

	public var exports: URL {
		root.appendingPathComponent("exports", isDirectory: true)
	}

	public init(root: URL) {
		self.root = root
	}
}

@MainActor public protocol SecretStore: AnyObject {
	func read(_ profile: UUID, ref: String) throws -> Data?
	func write(_ profile: UUID, ref: String, data: Data) throws
	func remove(_ profile: UUID, ref: String) throws
	func removeAll(_ profile: UUID) throws
	func randomBytes(count: Int) throws -> Data
}

@MainActor public protocol ProfileFileStore: AnyObject {
	func loadIndex() throws -> SessionIndex
	func saveIndex(_ index: SessionIndex) throws
	func paths(_ profile: UUID) throws -> ProfilePaths
	func databaseExists(_ profile: UUID) throws -> Bool
	func removeDatabase(_ profile: UUID) throws
	func removeExports(_ profile: UUID) throws
	func removeProfile(_ profile: UUID) throws
}
