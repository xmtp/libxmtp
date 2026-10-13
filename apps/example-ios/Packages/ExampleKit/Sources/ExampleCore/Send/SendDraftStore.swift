import Foundation

public enum SendPhase: String, Codable, Sendable {
	case queueing, accepted
}

public struct SendDraft: Identifiable, Codable, Equatable, Sendable {
	public var id: UUID
	public var conversationID: String
	public var phase: SendPhase
	public var messageID: String?
	public var created: Date
	public init(
		id: UUID = UUID(),
		conversationID: String,
		phase: SendPhase = .queueing,
		messageID: String? = nil,
		created: Date = Date(),
	) {
		self.id = id
		self.conversationID = conversationID
		self.phase = phase
		self.messageID = messageID
		self
			.created = created
	}
}

@MainActor public final class SendDraftStore {
	private let read: () throws -> [SendDraft]
	private let write: ([SendDraft]) throws -> Void
	public init(read: @escaping () throws -> [SendDraft], write: @escaping ([SendDraft]) throws -> Void) {
		self.read = read
		self.write = write
	}

	public func all() throws -> [SendDraft] {
		try read()
	}

	public func save(_ draft: SendDraft) throws {
		var values = try read()
		values.removeAll { $0.id == draft.id }
		values.append(draft)
		try write(values)
	}

	public func remove(_ id: UUID) throws {
		try write(read().filter { $0.id != id })
	}
}
