import Foundation

/// Removes a notification that arrives after its tag or all submissions were invalidated.
@MainActor public final class PushSubmissionFence {
	public struct Token: Equatable, Sendable {
		fileprivate let tag: String
		fileprivate let globalRevision: UInt64
		fileprivate let tagRevision: UInt64
		fileprivate let conversationID: String?
		public let conversationRevision: UInt64
	}

	private var globalRevision: UInt64 = 0
	private var tagRevisions: [String: UInt64] = [:]
	private var conversationRevisions: [String: UInt64] = [:]
	public init() {}
	public func capture(tag: String, conversationID: String? = nil) -> Token {
		Token(tag: tag, globalRevision: globalRevision, tagRevision: tagRevisions[tag, default: 0],
		      conversationID: conversationID, conversationRevision: revision(for: conversationID))
	}

	public func isCurrent(_ token: Token) -> Bool {
		token.globalRevision == globalRevision && token.tagRevision == tagRevisions[token.tag, default: 0] &&
			token.conversationRevision == revision(for: token.conversationID)
	}

	public func invalidate(tag: String) {
		tagRevisions[tag, default: 0] &+= 1
	}

	public func invalidate(conversationID: String) {
		conversationRevisions[conversationID, default: 0] &+= 1
	}

	public func shouldRemoveNotification(conversationID: String?, revision: UInt64?, before token: Token) -> Bool {
		guard isCurrent(token), let target = token.conversationID, target == conversationID else { return false }
		return revision != token.conversationRevision
	}

	private func revision(for conversationID: String?) -> UInt64 {
		conversationID.map { conversationRevisions[$0, default: 0] } ?? 0
	}

	public func invalidate() {
		globalRevision &+= 1
		tagRevisions.removeAll(); conversationRevisions.removeAll()
	}

	public func submit(
		expected: Token,
		add: @MainActor () async throws -> Void,
		remove: @MainActor () -> Void,
	) async -> Bool {
		guard isCurrent(expected) else { return false }
		do { try await add() } catch {
			if !isCurrent(expected) {
				remove()
			}
			return false
		}
		guard isCurrent(expected) else { remove(); return false }
		return true
	}
}
