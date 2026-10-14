import Foundation
import Observation

@MainActor public final class AttachmentDraftStore {
	private let names: () throws -> [String]
	private let read: (String) throws -> FileDraft?
	private let write: (String, FileDraft) throws -> Void
	private let remove: (String) throws -> Void
	private let references: () throws -> [String]
	private let saveReferences: ([String]) throws -> Void
	public init(
		names: @escaping () throws -> [String],
		read: @escaping (String) throws -> FileDraft?,
		write: @escaping (String, FileDraft) throws -> Void,
		remove: @escaping (String) throws -> Void,
		references: @escaping () throws -> [String],
		saveReferences: @escaping ([String]) throws -> Void,
	) {
		self.names = names
		self.read = read
		self.write = write
		self.remove = remove
		self.references = references
		self
			.saveReferences = saveReferences
	}

	public func all() throws -> [FileDraft] {
		try references().compactMap { try read($0) }
	}

	public func save(_ draft: FileDraft) throws {
		let key = "attachment-" + draft.id.uuidString
		try write(key, draft)
		var refs = try references()
		if !refs.contains(key) {
			refs.append(key)
			try saveReferences(refs)
		}
	}

	public func remove(_ draft: FileDraft) throws {
		let key = "attachment-" + draft.id.uuidString
		try remove(key)
		try saveReferences(references().filter { $0 != key })
	}

	public func cleanOrphans() throws {
		let refs = try Set(references())
		for key in try names() where !refs.contains(key) {
			try remove(key)
		}
		let surviving = try references().filter { try read($0) != nil }
		try saveReferences(surviving)
	}
}

public struct AttachmentState: Equatable, Sendable {
	public var limit: UInt64?
	public var drafts: [FileDraft] = []
	public var cards: [String: FileCard] =
		[:]
	public var error: String?
	public var busy = false
}

public enum AttachmentAction: Sendable {
	case load
	case stage(URL, String)
	case send(UUID)
	case discard(UUID)
	case assign(UUID, String)
	case download(String, FileDescriptor)
}

@MainActor @Observable public final class AttachmentStore {
	public private(set) var state = AttachmentState()
	public let verified = VerifiedFileCache()
	private let gateway: any AttachmentGateway
	private let staging: AttachmentStagingFiles?
	private let drafts: AttachmentDraftStore
	private let coordinator: SendCoordinator
	private let conversationID: String?
	private let origin: DestinationOrigin
	private let admit: (DestinationOrigin) -> Bool
	private let allowed: () -> Bool
	private let authorizeDownload: (() async throws -> Bool)?
	private let changed: () async -> Void
	private let tasks = InFlightTasks()
	private var downloadRevision: UInt64 = 0
	private var activeSends: Set<UUID> = []
	private var discardedIDs: Set<UUID> = []
	private var discardedFiles: [UUID: FileDescriptor] = [:]
	private var generations: [UUID: UInt64] = [:]

	public init(
		gateway: any AttachmentGateway,
		staging: AttachmentStagingFiles? = nil,
		drafts: AttachmentDraftStore,
		coordinator: SendCoordinator,
		conversationID: String?,
		origin: DestinationOrigin,
		admit: @escaping (DestinationOrigin) -> Bool,
		allowed: @escaping () -> Bool,
		changed: @escaping () async -> Void = {},
		authorizeDownload: (() async throws -> Bool)? = nil,
	) {
		self.gateway = gateway
		self.staging = staging
		self.drafts = drafts
		self.coordinator = coordinator
		self.conversationID = conversationID
		self.origin = origin
		self.admit = admit
		self.allowed = allowed
		self.changed = changed
		self.authorizeDownload = authorizeDownload
	}

	public func wait() async {
		await tasks.wait()
	}

	public func send(_ action: AttachmentAction) {
		if tasks.start({ await self.run(action) }) == nil, case let .stage(url, _) = action {
			do { try staging?.removeOwned(url) } catch {}
		}
	}

	public func invalidateDownloads() {
		downloadRevision &+= 1
		verified.clear()
		for id in state.cards.keys {
			state.cards[id]?.phase = .available
			state.cards[id]?.error = nil
			state.cards[id]?.retryable = false
		}
	}

	public func close() async {
		invalidateDownloads()
		await tasks.close()
	}

	private func apply(_ body: (inout AttachmentState) -> Void) {
		guard admit(origin) else { return }
		body(&state)
	}

	private func canSend(_ id: UUID, generation: UInt64) -> Bool {
		guard admit(origin), allowed(), !discardedIDs.contains(id),
		      generations[id, default: 0] == generation else { return false }
		do { return try drafts.all().contains { $0.id == id } } catch { return false }
	}

	private func run(_ action: AttachmentAction) async {
		let revision = downloadRevision
		do {
			switch action {
			case .load: try await load()
			case let .stage(url, name): try await stage(url, name: name)
			case let .assign(id, conversation): try assign(id, to: conversation)
			case let .discard(id): try await discard(id)
			case let .send(id): try await sendDraft(id)
			case let .download(id, descriptor): try await download(id, descriptor: descriptor)
			}
		} catch {
			if case .download = action, revision != downloadRevision {
				return
			}
			handleFailure(error, action: action)
		}
	}

	private func load() async throws {
		let limit = try await gateway.capability()
		let pending = try await gateway.pending()
		guard admit(origin) else { return }
		try drafts.cleanOrphans()
		var saved = try drafts.all()
		for descriptor in pending
			where !discardedFiles.values.contains(descriptor) && !saved.contains(where: { $0.descriptor == descriptor })
		{
			let draft = FileDraft(conversationID: nil, descriptor: descriptor)
			try drafts.save(draft)
			saved.append(draft)
		}
		for index in saved.indices where saved[index].phase == .uploading {
			saved[index].phase = .waiting
			try drafts.save(saved[index])
		}
		apply { $0.limit = limit; $0.drafts = saved }
	}

	private func stage(_ url: URL, name: String) async throws {
		do {
			guard allowed(), state.limit != nil, admit(origin) else { throw ChatError.stale }
			let descriptor = try await gateway.create(file: url, filename: name)
			try staging?.removeOwned(url)
			guard admit(origin) else { return }
			let draft = FileDraft(conversationID: conversationID, descriptor: descriptor)
			try drafts.save(draft)
			apply { $0.drafts.append(draft) }
		} catch {
			try staging?.removeOwned(url)
			throw error
		}
	}

	private func assign(_ id: UUID, to conversation: String) throws {
		guard var draft = try drafts.all().first(where: { $0.id == id }) else { return }
		draft.conversationID = conversation
		try drafts.save(draft)
		try reloadDrafts()
	}

	private func discard(_ id: UUID) async throws {
		guard let draft = try drafts.all().first(where: { $0.id == id }) else { return }
		generations[id, default: 0] += 1
		discardedIDs.insert(id)
		discardedFiles[id] = draft.descriptor
		do {
			try await gateway.discard(draft.descriptor)
			try drafts.remove(draft)
			apply { $0.drafts.removeAll { $0.id == id } }
		} catch {
			discardedIDs.remove(id)
			discardedFiles.removeValue(forKey: id)
			throw error
		}
	}

	private func sendDraft(_ id: UUID) async throws {
		guard !discardedIDs.contains(id), activeSends.insert(id).inserted else { return }
		defer { activeSends.remove(id) }
		let generation = generations[id, default: 0]
		guard allowed(), let draft = try drafts.all().first(where: { $0.id == id }),
		      let conversation = draft.conversationID, draft.phase != .unknown,
		      draft.phase != .failed || draft.retryable else { return }
		if let accepted = draft.acceptedID {
			try await coordinator.retry(
				conversationID: conversation,
				messageID: accepted,
				admission: { self.canSend(id, generation: generation) },
			)
		} else {
			guard let ready = try await uploadIfNeeded(draft, generation: generation) else { return }
			try await queue(ready, conversation: conversation, generation: generation)
		}
		try await finishSend(draft)
	}

	private func uploadIfNeeded(_ draft: FileDraft, generation: UInt64) async throws -> FileDraft? {
		if draft.phase == .complete {
			return draft
		}
		var next = draft
		next.phase = .uploading
		try drafts.save(next)
		try reloadDrafts()
		let phase = try await gateway.upload(next.descriptor)
		guard phase == .complete else { throw ChatError.unavailable }
		guard canSend(next.id, generation: generation) else { return nil }
		next.phase = .complete
		try drafts.save(next)
		return next
	}

	private func queue(_ draft: FileDraft, conversation: String, generation: UInt64) async throws {
		_ = try await coordinator.queue(
			draft: SendDraft(id: draft.id, conversationID: conversation), content: .attachment(draft.descriptor),
			admission: { self.canSend(draft.id, generation: generation) },
			acknowledge: { [drafts] id in
				var accepted = draft
				accepted.acceptedID = id
				accepted.phase = .complete
				try drafts.save(accepted)
			},
			beforeQueue: {
				var queueing = draft
				queueing.phase = .unknown
				try self.drafts.save(queueing)
			},
		)
	}

	private func finishSend(_ draft: FileDraft) async throws {
		guard admit(origin) else { return }
		let current = try drafts.all().first { $0.id == draft.id }
		if current?.acceptedID == nil {
			try reloadDrafts(); return
		}
		try drafts.remove(draft)
		apply { $0.drafts.removeAll { $0.id == draft.id } }
		await changed()
	}

	private func download(_ id: String, descriptor: FileDescriptor) async throws {
		let revision = downloadRevision
		guard allowed(), admit(origin), state.cards[id]?.phase != .downloading else { return }
		if let authorizeDownload {
			guard try await authorizeDownload() else { return }
		}
		try Task.checkCancellation()
		guard revision == downloadRevision, allowed(), admit(origin), state.cards[id]?.phase != .downloading else { return }
		apply { $0.cards[id] = FileCard(id: id, name: descriptor.filename ?? "File", length: descriptor.length,
		                                phase: .downloading) }
		let file = try await gateway.download(descriptor)
		guard revision == downloadRevision, admit(origin), allowed() else { return }
		let evicted = verified.put(id, file: file)
		apply { state in
			for id in evicted {
				state.cards[id]?.phase = .available
			}
			state.cards[id] = FileCard(id: id, name: file.filename, length: descriptor.length, phase: .verified)
		}
	}

	private func reloadDrafts() throws {
		let values = try drafts.all()
		apply { $0.drafts = values }
	}

	private func handleFailure(_ error: any Error, action: AttachmentAction) {
		let failure = error as? AttachmentError
		if case let .send(id) = action {
			recordSendFailure(id, failure: failure)
		}
		let message = failure?.message ?? "The file operation failed. Try again."
		apply { $0.error = message }
		if case let .download(id, descriptor) = action {
			apply { $0.cards[id] = FileCard(id: id, name: descriptor.filename ?? "File", length: descriptor.length,
			                                phase: .failed, error: message, retryable: failure?.retryable ?? true) }
		}
	}

	private func recordSendFailure(_ id: UUID, failure: AttachmentError?) {
		do {
			if var draft = try drafts.all().first(where: { $0.id == id }), draft.phase == .uploading {
				draft.phase = .failed
				draft.retryable = failure?.retryable ?? true
				try drafts.save(draft)
			}
			try reloadDrafts()
		} catch {}
	}
}
