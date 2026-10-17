@testable import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func completedFeatureTasksAreReleased() async {
		let tasks = InFlightTasks()
		for _ in 0 ..< 100 {
			tasks.start {}
		}
		await tasks.wait()
		#expect(tasks.count == 0)
		tasks.start { while !Task.isCancelled {
			await Task.yield()
		} }
		await tasks.close()
		#expect(tasks.count == 0)
	}

	@Test func photoImportsKeepTheirOwnContext() async throws {
		let imports = AttachmentImportGate<String>()
		let pause = ReviewGate()
		let first = Task {
			try await imports.withContext("first-profile") { await pause.wait(); return await imports.current }
		}
		for _ in 0 ..< 10000 {
			if pause.started {
				break
			}; await Task.yield()
		}
		try #require(pause.started)
		let second = Task { try await imports.withContext("second-profile") { await imports.current } }
		for _ in 0 ..< 100 {
			await Task.yield()
		}
		#expect(await imports.current == "first-profile")
		pause.release()
		#expect(try await first.value == "first-profile")
		#expect(try await second.value == "second-profile")
	}

	@Test func failedInitialDraftSaveRemainsEditable() async {
		var saved = ComposerDraft()
		var fail = true
		var records: [SendDraft] = []
		let queries = ChatQueryFake()
		let drafts = ComposerDraftStore(read: { _ in saved }, write: { _, value in
			if fail, value.requestID != nil {
				throw ChatError.storage
			}
			saved = value
		})
		let sends = SendDraftStore(read: { records }, write: { records = $0 })
		let coordinator = SendCoordinator(drafts: sends, queries: queries, accepts: { true })
		let composer = ComposerStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			drafts: drafts,
			sends: sends,
			coordinator: coordinator,
			admit: { _ in true },
		)
		composer.send(.allow(true)); composer.send(.edit("kept")); composer.send(.submit)
		#expect(composer.state.enabled); #expect(composer.state.draft.requestID == nil); #expect(queries.queues == 0)
		fail = false
		composer.send(.submit); await composer.wait()
		#expect(queries.queues == 1)
	}

	@Test func knownPreflightFailureReleasesPersistedComposer() async {
		var saved = ComposerDraft()
		var records: [SendDraft] = []
		let queries = ChatQueryFake()
		queries.conversationHook = { throw ChatError.unavailable }
		let drafts = ComposerDraftStore(read: { _ in saved }, write: { _, value in saved = value })
		let sends = SendDraftStore(read: { records }, write: { records = $0 })
		let coordinator = SendCoordinator(drafts: sends, queries: queries, accepts: { true })
		let origin = DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1)
		let composer = ComposerStore(
			id: "chat",
			origin: origin,
			drafts: drafts,
			sends: sends,
			coordinator: coordinator,
			admit: { _ in true },
		)
		composer.send(.allow(true)); composer.send(.edit("kept")); composer.send(.submit); await composer.wait()
		#expect(saved.requestID == nil); #expect(saved.text == "kept")
		queries.conversationHook = nil
		let reopened = ComposerStore(
			id: "chat",
			origin: origin,
			drafts: drafts,
			sends: sends,
			coordinator: coordinator,
			admit: { _ in true },
		)
		reopened.send(.allow(true)); #expect(reopened.state.enabled)
		reopened.send(.submit); await reopened.wait(); #expect(queries.queues == 1)
	}

	@Test(arguments: [false, true]) func previewWaitersFinishAfterInvalidation(clear: Bool) async throws {
		let cache = PreviewCache<Int>()
		let pauses = (0 ..< 3).map { _ in ReviewGate() }
		let active = pauses.enumerated()
			.map { index, pause in
				Task { await cache.load(id: "active-" + String(index)) { await pause.wait(); return index } }
			}
		for _ in 0 ..< 10000 {
			if pauses.allSatisfy(\.started) {
				break
			}; await Task.yield()
		}
		let allStarted = pauses.allSatisfy(\.started)
		try #require(allStarted)
		var completed = 0
		let pending = (0 ..< 5)
			.map { index in Task { await cache.load(id: "pending-" + String(index)) { index }; completed += 1 } }
		for _ in 0 ..< 10000 {
			if cache.waitingCount == 5 {
				break
			}; await Task.yield()
		}
		try #require(cache.waitingCount == 5)
		if clear {
			cache.clear()
		} else {
			pending.forEach { $0.cancel() }
		}
		pauses.forEach { $0.release() }
		for _ in 0 ..< 10000 {
			if completed == 5 {
				break
			}; await Task.yield()
		}
		#expect(completed == 5)
		pending.forEach { $0.cancel() }
		for task in active {
			await task.value
		}
	}
}

extension MessageMapperTests {
	@MainActor @Test func disjointNewestRefreshReplacesStaleWindow() async {
		let queries = ChatQueryFake()
		queries.rows = (1 ... 50).reversed().map { ChatMessage(
			id: String($0),
			conversationID: "chat",
			sender: "peer",
			sentAt: Int64($0),
			content: .text("old"),
			position: HistoryPosition(sentAt: Int64($0), cursor: String($0)),
		) }
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			queries: queries,
			admit: { _ in true },
		)
		timeline.send(.load); await timeline.wait()
		queries.rows = (51 ... 100).reversed().map { ChatMessage(
			id: String($0),
			conversationID: "chat",
			sender: "peer",
			sentAt: Int64($0),
			content: .text("new"),
			position: HistoryPosition(sentAt: Int64($0), cursor: String($0)),
		) }
		await timeline.refresh()
		#expect(timeline.state.window.rows.first?.id == "100")
		#expect(timeline.state.window.rows.last?.id == "51")
	}
}
