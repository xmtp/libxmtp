@testable import ExampleCore
import Foundation
import Testing

@MainActor struct SendCoordinatorTests {
	@Test func persistsBeforeQueueAndRetriesOnlyPublication() async throws {
		var stored: [SendDraft] = []; let queries = ChatQueryFake()
		let drafts = SendDraftStore(read: { stored }, write: { stored = $0 })
		queries.beforeQueue = { #expect(stored.count == 1); #expect(stored.first?.phase == .queueing) }
		let coordinator = SendCoordinator(drafts: drafts, queries: queries, accepts: { true })
		let id = try await coordinator.queue(
			draft: SendDraft(conversationID: "chat"),
			content: .text("hello"),
			admission: { true },
		)
		try await coordinator.retry(conversationID: "chat", messageID: id)
		#expect(queries.queues == 1); #expect(queries.publications == 2)
	}

	@Test func retainsAcceptedIDWhenDurableWriteFails() async throws {
		var stored: [SendDraft] = []; var writes = 0; let queries = ChatQueryFake()
		let drafts = SendDraftStore(
			read: { stored },
			write: { value in
				writes += 1; if writes == 2 {
					throw ChatError.storage
				}; stored = value
			},
		)
		let coordinator = SendCoordinator(drafts: drafts, queries: queries, accepts: { true })
		let id = try await coordinator.queue(
			draft: SendDraft(conversationID: "chat"),
			content: .text("hello"),
			admission: { true },
		)
		#expect(id == "accepted"); #expect(queries.publications == 0)
		try coordinator.recoverAccepted()
		#expect(stored.first?.messageID == id); #expect(stored.first?.phase == .accepted)
		try await coordinator.retry(conversationID: "chat", messageID: id)
		#expect(queries.queues == 1); #expect(queries.publications == 1)
	}

	@Test func restartWithUnknownOutcomeLocksComposer() {
		var stored = [SendDraft(conversationID: "chat")]; var saved = ComposerDraft(); saved.edit("kept")
		let drafts = SendDraftStore(read: { stored }, write: { stored = $0 }); let queries = ChatQueryFake()
		let composer = ComposerStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			drafts: ComposerDraftStore(read: { _ in saved }, write: { _, value in saved = value }),
			sends: drafts,
			coordinator: SendCoordinator(drafts: drafts, queries: queries, accepts: { true }),
			admit: { _ in true },
		)
		composer.send(.allow(true)); composer.send(.edit("replacement")); composer.send(.submit)
		#expect(!composer.state.enabled); #expect(composer.state.unknown); #expect(composer.state.draft
			.text == "kept"); #expect(queries.queues == 0)
	}

	@Test func attachmentRetryDoesNotQueueAgain() async throws {
		var stored: [SendDraft] = []; let queries = ChatQueryFake(); let drafts = SendDraftStore(
			read: { stored },
			write: { stored = $0 },
		)
		let coordinator = SendCoordinator(drafts: drafts, queries: queries, accepts: { true })
		let file = FileDescriptor(
			url: "https://example.test/file",
			digest: "digest",
			secret: Data(),
			salt: Data(),
			nonce: Data(),
			scheme: "https",
			length: 5,
			filename: "test.txt",
		)
		let id = try await coordinator.queue(
			draft: SendDraft(conversationID: "chat"),
			content: .attachment(file),
			admission: { true },
		)
		try await coordinator.retry(conversationID: "chat", messageID: id)
		#expect(queries.queues == 1); #expect(queries.publications == 2)
	}
}

@MainActor final class AttachmentGatewayFake: AttachmentGateway {
	var downloads = 0
	var uploads = 0
	var discardHook: (() async throws -> Void)?
	var uploadHook: (() async throws -> Void)?
	var createHook: ((URL, String) async throws -> FileDescriptor)?
	func capability() async throws -> UInt64? {
		1024
	}

	func create(file: URL, filename: String) async throws -> FileDescriptor {
		guard let createHook else { throw ChatError.unavailable }
		return try await createHook(file, filename)
	}

	func pending() async throws -> [FileDescriptor] {
		[]
	}

	func upload(_: FileDescriptor) async throws -> FilePhase {
		uploads += 1; try await uploadHook?(); return .complete
	}

	func download(_: FileDescriptor) async throws -> VerifiedFile {
		downloads += 1
		return VerifiedFile(
			url: URL(fileURLWithPath: "/tmp/test"),
			mimeType: "image/png",
			filename: "test.png",
		)
	}

	func discard(_: FileDescriptor) async throws {
		try await discardHook?()
	}
}

extension SendCoordinatorTests {
	@Test func acceptedAttachmentRetriesWithoutUpload() async {
		let file = FileDescriptor(
			url: "https://example.test/file",
			digest: "digest",
			secret: Data(),
			salt: Data(),
			nonce: Data(),
			scheme: "https",
			length: 5,
			filename: "test.txt",
		)
		let draft = FileDraft(conversationID: "chat", descriptor: file, phase: .complete, acceptedID: "accepted")
		var saved = ["attachment-" + draft.id.uuidString: draft]
		var refs = Array(saved.keys)
		let drafts = AttachmentDraftStore(
			names: { Array(saved.keys) },
			read: { saved[$0] },
			write: { saved[$0] = $1 },
			remove: { saved.removeValue(forKey: $0) },
			references: { refs },
			saveReferences: { refs = $0 },
		)
		var sends: [SendDraft] = []
		let sendDrafts = SendDraftStore(read: { sends }, write: { sends = $0 })
		let queries = ChatQueryFake(); let gateway = AttachmentGatewayFake()
		let coordinator = SendCoordinator(drafts: sendDrafts, queries: queries, accepts: { true })
		let store = AttachmentStore(
			gateway: gateway,
			drafts: drafts,
			coordinator: coordinator,
			conversationID: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			admit: { _ in true },
			allowed: { true },
		)
		store.send(.send(draft.id)); await store.wait()
		#expect(gateway.uploads == 0); #expect(queries.queues == 0); #expect(queries.publications == 1); #expect(saved
			.isEmpty)
	}
}

@MainActor final class AttachmentFixture {
	let queries = ChatQueryFake()
	let gateway = AttachmentGatewayFake()
	var saved: [String: FileDraft] = [:]
	var refs: [String] = []
	var sends: [SendDraft] = []
	let file = FileDescriptor(
		url: "https://example.test/file",
		digest: "digest",
		secret: Data(),
		salt: Data(),
		nonce: Data(),
		scheme: "https",
		length: 5,
		filename: "test.png",
	)
	lazy var drafts = AttachmentDraftStore(
		names: { Array(self.saved.keys) },
		read: { self.saved[$0] },
		write: { self.saved[$0] = $1 },
		remove: { self.saved.removeValue(forKey: $0) },
		references: { self.refs },
		saveReferences: { self.refs = $0 },
	)
	lazy var coordinator = SendCoordinator(
		drafts: SendDraftStore(read: { self.sends }, write: { self.sends = $0 }),
		queries: queries,
		accepts: { true },
	)
	lazy var store = AttachmentStore(
		gateway: gateway,
		drafts: drafts,
		coordinator: coordinator,
		conversationID: "chat",
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in true },
		allowed: { true },
	)
	func add(phase: FilePhase = .waiting) throws -> FileDraft {
		let draft = FileDraft(
			conversationID: "chat",
			descriptor: file,
			phase: phase,
		); try drafts.save(draft); return draft
	}
}

@MainActor final class ReviewGate {
	var started = false
	private var continuation: CheckedContinuation<Void, Never>?
	private var released = false
	func wait() async {
		started = true; if released {
			return
		}; await withCheckedContinuation { continuation = $0 }
	}

	func release() {
		released = true; continuation?.resume(); continuation = nil
	}
}

extension SendCoordinatorTests {
	@Test func acceptedCommitFailureCannotSubmitTwice() async throws {
		var stored: [SendDraft] = []; var saved = ComposerDraft(); var fail = true
		let queries = ChatQueryFake()
		let sends = SendDraftStore(
			read: { stored },
			write: { values in
				if fail, values.contains(where: { $0.phase == .accepted }) {
					throw ChatError.storage
				}; stored = values
			},
		)
		let coordinator = SendCoordinator(drafts: sends, queries: queries, accepts: { true })
		let composer = ComposerStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			drafts: ComposerDraftStore(read: { _ in saved }, write: { _, value in saved = value }),
			sends: sends,
			coordinator: coordinator,
			admit: { _ in true },
		)
		composer.send(.allow(true)); composer.send(.edit("hello")); composer.send(.submit); await composer.wait()
		#expect(!composer.state.enabled); #expect(composer.state.draft.acceptedID == "accepted")
		composer.send(.submit); await composer.wait(); #expect(queries.queues == 1)
		fail = false; try coordinator.recoverAccepted()
		#expect(composer.state.draft.text.isEmpty); #expect(composer.state.enabled)
	}

	@Test func acceptedRestartAcknowledgesTheSubmittedRevision() async {
		var saved = ComposerDraft(); saved.edit("original"); let request = UUID(); saved.requestID = request; saved
			.submittedRevision = saved.revision
		var stored = [SendDraft(id: request, conversationID: "chat", phase: .accepted, messageID: "accepted")]
		let queries = ChatQueryFake(); let sends = SendDraftStore(read: { stored }, write: { stored = $0 })
		let composer = ComposerStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			drafts: ComposerDraftStore(read: { _ in saved }, write: { _, value in saved = value }),
			sends: sends,
			coordinator: SendCoordinator(drafts: sends, queries: queries, accepts: { true }),
			admit: { _ in true },
		)
		composer
			.send(.allow(true)); #expect(composer.state.draft.text.isEmpty); #expect(composer.state.draft.requestID == nil)
		composer.send(.submit); await composer.wait(); #expect(queries.queues == 0)
	}

	@Test func interruptedUploadCanResumeWithoutUnknownSend() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(phase: .uploading)
		fixture.store.send(.load); await fixture.store.wait()
		#expect(fixture.store.state.drafts.first?.phase == .waiting)
		fixture.store.send(.send(draft.id)); await fixture.store.wait()
		#expect(fixture.gateway.uploads == 1); #expect(fixture.queries.queues == 1)
	}

	@Test func preflightFailureKeepsCompletedUploadForRetry() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add()
		fixture.queries.conversationHook = { throw ChatError.unavailable }
		fixture.store.send(.send(draft.id)); await fixture.store.wait()
		#expect(try fixture.drafts.all().first?.phase == .complete); #expect(fixture.queries.queues == 0)
		fixture.queries.conversationHook = nil
		fixture.store.send(.send(draft.id)); await fixture.store.wait()
		#expect(fixture.gateway.uploads == 1); #expect(fixture.queries.queues == 1)
	}

	@Test(arguments: [true, false]) func discardDuringAwaitCannotQueue(atUpload: Bool) async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let gate = ReviewGate()
		if atUpload {
			fixture.gateway.uploadHook = { await gate.wait() }
		} else {
			fixture.queries.conversationHook = { await gate.wait() }
		}
		fixture.store.send(.send(draft.id))
		for _ in 0 ..< 10000 {
			if gate.started {
				break
			}; await Task.yield()
		}
		try #require(gate.started)
		fixture.store.send(.discard(draft.id))
		for _ in 0 ..< 10000 {
			if fixture.saved.isEmpty {
				break
			}; await Task.yield()
		}
		gate.release(); await fixture.store.wait()
		#expect(fixture.queries.queues == 0); #expect(fixture.queries.publications == 0)
	}

	@Test func evictedVerifiedFileBecomesDownloadableAgain() async {
		let fixture = AttachmentFixture()
		for index in 0 ..< 17 {
			fixture.store.send(.download(String(index), fixture.file)); await fixture.store.wait()
		}
		#expect(fixture.store.verified.get("0") == nil)
		#expect(fixture.store.state.cards["0"]?.phase == .available)
		#expect(fixture.store.state.cards["16"]?.phase == .verified)
	}

	@Test func terminalUploadFailureCannotRetry() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add()
		fixture.gateway.uploadHook = { throw AttachmentError.failure(message: "Unavailable", retryable: false) }
		fixture.store.send(.send(draft.id)); await fixture.store.wait()
		#expect(try fixture.drafts.all().first?.retryable == false)
		fixture.store.send(.send(draft.id)); await fixture.store.wait()
		#expect(fixture.gateway.uploads == 1); #expect(fixture.queries.queues == 0)
	}

	@Test func decodedPreviewRetentionIsBounded() async {
		let cache = PreviewCache<Int>()
		for index in 0 ..< 6 {
			await cache.load(id: String(index)) { index }; #expect(cache.values.count <= 3)
		}
		#expect(cache.values["0"] == nil); #expect(cache.values["5"] == 5)
	}
}
