import Foundation

/// Operations consume their SDK stream directly and await each projection before reading again.
/// The owning session runs this method in its scope and cancels and awaits it before ending its client.
public struct SessionReaders: Sendable {
	public typealias Reader = @Sendable () async throws -> Void
	private let messages: Reader
	private let conversations: Reader
	private let events: Reader
	public init(messages: @escaping Reader, conversations: @escaping Reader, events: @escaping Reader) {
		self.messages = messages; self.conversations = conversations; self.events = events
	}

	public func run() async throws {
		try await withThrowingTaskGroup(of: Void.self) { group in
			group.addTask(operation: messages); group.addTask(operation: conversations); group.addTask(operation: events)
			defer { group.cancelAll() }
			while try await group.next() != nil {}
		}
	}
}

public enum SessionEvent: Sendable {
	case handleChanged(inboxID: String, handle: String?, otherHandles: [String])
	case messageDeleted(conversationID: String, messageID: String, locallyDeleted: Bool)
	case changed
}
