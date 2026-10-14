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

	public var acceptsSession: Bool {
		accepts()
	}

	public func publicationConfirmed(conversationID: String, messageID: String) async throws -> Bool {
		guard accepts() else { return false }
		let message = try await queries.message(id: messageID)
		guard accepts(), message?.conversationID == conversationID else { return false }
		return message?.delivery == .published
	}

	public func queue(
		draft: SendDraft,
		content: OutboundContent,
		admission: @escaping () -> Bool,
		acknowledge: @escaping (String) throws -> Void = { _ in },
		beforeQueue: () throws -> Void = {},
	) async throws -> String {
		try await queue(draft: draft, admission: admission, acknowledge: acknowledge, beforeQueue: beforeQueue) {
			try await self.queries.queue(conversationID: draft.conversationID, content: content)
		}
	}

	public func queue(
		draft: SendDraft,
		admission: @escaping () -> Bool,
		acknowledge: @escaping (String) throws -> Void = { _ in },
		beforeQueue: () throws -> Void = {},
		accept: () async throws -> String,
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
		let id: String
		do { id = try await accept() } catch let error as ChatError
			where error == .rejected || error == .unavailable || error == .stale
		{
			try restore(previous, replacing: draft)
			throw error
		}
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

	public func delete(conversationID: String, messageID: String, admission: @escaping () -> Bool) async throws -> String {
		guard accepts(), admission() else { throw ChatError.stale }
		try recoverAccepted()
		if let saved = try drafts.all().first(where: { $0.deletionTargetID == messageID }) {
			guard saved.conversationID == conversationID, saved.phase == .accepted, let id = saved.messageID else {
				throw ChatError.duplicate
			}
			do { try await retry(conversationID: conversationID, messageID: id, admission: admission) } catch { return id }
			return id
		}
		return try await queue(draft: SendDraft(conversationID: conversationID, deletionTargetID: messageID),
		                       admission: admission)
		{
			try await self.queries.delete(messageID: messageID)
		}
	}

	public func deletionNeedsReview(messageID: String) throws -> Bool {
		try drafts.all().contains { $0.deletionTargetID == messageID }
	}

	private func restore(_ previous: SendDraft?, replacing draft: SendDraft) throws {
		if let previous {
			try drafts.save(previous)
		} else {
			try drafts.remove(draft.id)
		}
	}

	public func acceptedMessageID(for draftID: UUID, conversationID: String) throws -> String? {
		try drafts.all().first {
			$0.id == draftID && $0.conversationID == conversationID && $0.phase == .accepted
		}?.messageID
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
