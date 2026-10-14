import Foundation

public struct ComposerDraft: Codable, Equatable, Sendable {
	public var text = ""
	public var revision: UInt64 =
		0
	public var replyID: String?
	public var requestID: UUID?
	public var submittedRevision: UInt64?
	public var acceptedID: String?
	public init() {}
	public mutating func edit(_ value: String) {
		if text != value {
			text = value
			revision += 1
		}
	}

	public mutating func release() {
		requestID = nil
		submittedRevision = nil
		acceptedID = nil
	}
}

@MainActor public final class ComposerDraftStore {
	private let read: (String) throws -> ComposerDraft?
	private let write: (String, ComposerDraft) throws -> Void
	public init(
		read: @escaping (String) throws -> ComposerDraft?,
		write: @escaping (String, ComposerDraft) throws -> Void,
	) {
		self.read = read
		self.write = write
	}

	public func load(_ id: String) throws -> ComposerDraft {
		try read(id) ?? ComposerDraft()
	}

	public func save(_ value: ComposerDraft, for id: String) throws {
		try write(id, value)
	}
}
