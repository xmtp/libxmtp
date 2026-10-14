import Foundation
import Observation

public struct ComposerState: Equatable, Sendable {
	public var draft = ComposerDraft()
	public var sending = false
	public var unknown = false
	public var allowed =
		false
	public var error: String?
	public var enabled: Bool {
		allowed && !sending && !unknown && draft.requestID == nil
	}
}

public enum ComposerAction: Sendable {
	case edit(String), reply(String?), submit, allow(Bool), clearReview, reload
}

@MainActor @Observable public final class ComposerStore {
	public private(set) var state = ComposerState()
	private let id: String
	private let origin: DestinationOrigin
	private let admit: (DestinationOrigin) -> Bool
	private let drafts: ComposerDraftStore
	private let sends: SendDraftStore
	private let coordinator: SendCoordinator
	private let accepted: (
	) async
		-> Void
	private var job: Task<Void, Never>?
	public init(
		id: String,
		origin: DestinationOrigin,
		drafts: ComposerDraftStore,
		sends: SendDraftStore,
		coordinator: SendCoordinator,
		admit: @escaping (DestinationOrigin) -> Bool,
		accepted: @escaping () async -> Void = {},
	) {
		self.id = id
		self.origin = origin
		self.drafts = drafts
		self.sends = sends
		self.coordinator = coordinator
		self
			.admit = admit
		self.accepted = accepted
		reload()
	}

	public func send(_ action: ComposerAction) {
		guard admit(origin) else { return }
		switch action {
		case .reload: reload()
		case let .allow(allowed): state.allowed = allowed
		case let .edit(text): guard state.enabled else { return }
			state.draft.edit(text)
			persist()
		case let .reply(id): guard state.enabled else { return }
			state.draft.replyID = id
			persist()
		case .clearReview: clearReview()
		case .submit: guard state.enabled,
		                    !state.draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
			else { return }
			submit()
		}
	}

	private func clearReview() {
		do {
			for draft in try sends.all()
				where draft.conversationID == id && draft.phase == .queueing
			{
				try sends.remove(draft.id)
			}
			state.draft
				.release()
			state.unknown = false
			persist()
		} catch { state.error = "Cannot clear saved send status." }
	}

	public func wait() async {
		await job?.value
	}

	public func close() async {
		job?.cancel()
		await job?.value
	}

	private func reload() {
		do {
			var saved = try drafts.load(id)
			let references = try sends.all().filter { $0.conversationID == id }
			if let reference = references.first(where: { $0.id == saved.requestID && $0.phase == .accepted }),
			   let acceptedID = reference.messageID
			{
				saved.acceptedID = acceptedID
				if saved.revision == saved.submittedRevision {
					saved.edit("")
					saved.replyID = nil
				}
				saved.release()
				try drafts.save(saved, for: id)
			}
			if !state.sending, let request = saved.requestID,
			   !references.contains(where: { $0.id == request })
			{
				saved.release()
				try drafts.save(saved, for: id)
			}
			state.draft = saved
			state.unknown = references.contains { $0.phase == .queueing && $0.messageID == nil }
		} catch { state.unknown = true; state.error = "Cannot load your saved draft." }
	}

	private func persist() {
		do { try drafts.save(state.draft, for: id) } catch { state.error = "Cannot save your draft. Try again." }
	}

	private func apply(_ body: (inout ComposerState) -> Void) {
		guard admit(origin) else { return }
		body(&state)
	}

	private static func acknowledge(
		_ messageID: String,
		request: UUID,
		submitted: ComposerDraft,
		id: String,
		drafts: ComposerDraftStore,
	) throws -> ComposerDraft? {
		var current = try drafts.load(id)
		guard current.requestID == request, current.submittedRevision == submitted.revision else { return nil }
		current.acceptedID = messageID
		try drafts.save(current, for: id)
		if current.revision == submitted.revision {
			current.edit("")
			if current.replyID == submitted.replyID {
				current.replyID = nil
			}
		}
		current.release()
		try drafts.save(current, for: id)
		return current
	}

	private func releaseReservation(_ request: UUID) throws -> ComposerDraft? {
		var saved = try drafts.load(id)
		guard saved.requestID == request else { return nil }
		saved.release()
		try drafts.save(saved, for: id)
		return saved
	}

	private func failSubmission(_ request: UUID) {
		let unknown: Bool
		do { unknown = try sends.all().contains { $0.id == request && $0.phase == .queueing } } catch { unknown = true }
		let released: ComposerDraft?
		do { released = unknown ? nil : try releaseReservation(request) } catch {
			apply { $0.sending = false; $0.error = "Cannot save your draft. Try again." }; return
		}
		apply {
			if let released {
				$0.draft = released
			}
			$0.sending = false
			$0.unknown = unknown
			$0
				.error = unknown ? "Send outcome unknown. The original message may already exist." :
				"Cannot send this message. Your draft is kept."
			if !unknown {
				$0.draft.release()
			}
		}
	}

	private func submit() {
		let request = UUID()
		let submitted = state.draft
		let send = SendDraft(id: request, conversationID: id)
		state.draft.requestID = request
		state.draft.submittedRevision = submitted.revision
		do { try drafts.save(state.draft, for: id) } catch { state.draft = submitted
			state.error = "Cannot save your draft. Try again."
			return
		}
		state.sending = true
		state.error = nil
		job = Task {
			do {
				let content: OutboundContent = submitted.replyID
					.map { .reply(parent: $0, text: submitted.text) } ?? .text(submitted.text)
				// A reopened composer may have released this reservation before this task started.
				guard try drafts.load(id).requestID == request else {
					apply { $0.sending = false }
					return
				}
				let messageID = try await coordinator.queue(
					draft: send,
					content: content,
					admission: { self.admit(self.origin) },
					acknowledge: { [drafts, id, weak owner = self] messageID in
						if let current = try Self
							.acknowledge(messageID, request: request, submitted: submitted, id: id, drafts: drafts)
						{
							owner?.apply { $0.draft = current }
						}
					},
				)
				apply {
					$0.sending = false
					$0.unknown = false
					if $0.draft
						.requestID != nil
					{
						$0.draft.acceptedID = messageID
						$0.error = "Send accepted. Local recovery is pending."
					}
				}
				await accepted()
			} catch { failSubmission(request) }
		}
	}
}
