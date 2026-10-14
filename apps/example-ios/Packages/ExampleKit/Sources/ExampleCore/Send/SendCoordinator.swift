import Foundation

/// Acceptance commits survive navigation. Visible state still uses the destination fence.
@MainActor public final class SendCoordinator {
	private let drafts: SendDraftStore
	private let queries: any ChatQueries
	private let accepts: () -> Bool
	private var active: Set<UUID> = []
	private var known: [UUID: AcceptedMessageCommit] = [:]
	public init(drafts: SendDraftStore, queries: any ChatQueries, accepts: @escaping () -> Bool) {
		self.drafts = drafts
		self.queries = queries
		self.accepts = accepts
	}

	public func queue(
		draft: SendDraft,
		content: OutboundContent,
		admission: @escaping () -> Bool,
		acknowledge: @escaping (String) throws -> Void = { _ in },
		beforeQueue: () throws -> Void = {},
	) async throws -> String {
		guard accepts(),
		      admission() else { throw ChatError.stale }
		guard active.insert(draft.id).inserted
		else { throw ChatError.duplicate }
		defer { active.remove(draft.id) }
		let previous = try drafts.all().first { $0.id == draft.id }
		try drafts.save(draft)
		let conversation: ConversationRecord
		do { conversation = try await queries.conversation(id: draft.conversationID) } catch {
			try restore(previous, replacing: draft)
			throw error
		}
		guard accepts(), admission(),
		      conversation.consent == .allowed
		else {
			try restore(previous, replacing: draft)
			throw ChatError.stale
		}
		do { try beforeQueue() } catch {
			try restore(previous, replacing: draft)
			throw error
		}
		let id = try await queries.queue(conversationID: draft.conversationID, content: content)
		var accepted = draft
		accepted.phase = .accepted
		accepted.messageID = id
		let saved = accepted
		let commit = AcceptedMessageCommit(
			messageID: id,
			persist: { [drafts] in try drafts.save(saved) },
			verify: { [drafts] in try drafts.all().contains(saved) },
			acknowledge: { try acknowledge(id) },
		)
		known[draft.id] = commit
		do { try commit.finish()
			known.removeValue(forKey: draft.id)
		} catch { return id }
		guard accepts(), admission() else { return id }
		// Acceptance is durable. A publication failure remains recoverable by its accepted message ID.
		do { try await retry(conversationID: draft.conversationID, messageID: id, admission: admission) } catch { return id }
		return id
	}

	private func restore(_ previous: SendDraft?, replacing draft: SendDraft) throws {
		if let previous {
			try drafts.save(previous)
		} else {
			try drafts.remove(draft.id)
		}
	}

	public func recoverAccepted() throws {
		for (id, commit) in known {
			try commit.finish()
			known.removeValue(forKey: id)
		}
	}

	public func retry(conversationID: String, messageID: String, admission: @escaping () -> Bool = { true }) async throws {
		guard accepts(), admission() else { throw ChatError.stale }
		for (id, commit) in known where commit.messageID == messageID {
			try commit.finish()
			known.removeValue(forKey: id)
		}
		guard let message = try await queries.message(id: messageID),
		      message.conversationID == conversationID else { throw ChatError.unavailable }
		let conversation = try await queries.conversation(id: conversationID)
		guard accepts(), admission(), conversation.consent == .allowed else { throw ChatError.stale }
		if message.delivery != .published {
			try await queries.publish(conversationID: conversationID, messageID: messageID)
		}
		guard accepts() else { return }
		for draft in try drafts.all() where draft.messageID == messageID {
			try drafts.remove(draft.id)
		}
	}
}
