import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func currentConsentIsCheckedBeforeDownload() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add()
		let files = AttachmentStore(
			gateway: fixture.gateway,
			drafts: fixture.drafts,
			coordinator: fixture.coordinator,
			conversationID: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			admit: { _ in true },
			allowed: { true },
			authorizeDownload: { false },
		)
		files.send(.download("message", draft.descriptor)); await files.wait()
		#expect(fixture.gateway.downloads == 0); #expect(files.verified.get("message") == nil)
		await files.close()
	}

	@Test func pendingRecoveryClearsEarlierError() async {
		let queries = ChatQueryFake(); var first = true
		queries.pendingHook = { _ in
			defer { first = false }
			return PendingPage(rows: [], last: nil, hasMore: false, skipped: first ? 1 : 0)
		}
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			queries: queries,
			admit: { _ in true },
		)
		timeline.send(.newestPending); await timeline.wait(); #expect(timeline.state.error != nil)
		timeline.send(.newestPending); await timeline.wait(); #expect(timeline.state.error == nil)
		await timeline.close()
	}

	@Test func failedAcceptedCommitDoesNotRetainCoordinator() async throws {
		var saved: [SendDraft] = []; weak var released: SendCoordinator?
		let drafts = SendDraftStore(read: { saved }, write: { rows in
			if rows.contains(where: { $0.phase == .accepted }) {
				throw ChatError.unavailable
			}
			saved = rows
		})
		func queue() async throws {
			let coordinator = SendCoordinator(drafts: drafts, queries: ChatQueryFake(), accepts: { true })
			released = coordinator
			_ = try await coordinator.queue(
				draft: SendDraft(conversationID: "chat"),
				content: .text("kept"),
				admission: { true },
			)
		}
		try await queue(); #expect(released == nil)
	}

	@Test func mediaReloadClosesAuthorizationWhileConsentIsStale() async throws {
		let queries = ChatQueryFake(); let gate = ReviewGate()
		let media = SharedMediaStore(
			id: "chat",
			queries: queries,
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			admit: { _ in true },
		)
		media.send(.load)
		for _ in 0 ..< 10000 {
			if !media.state.loading {
				break
			}; await Task.yield()
		}
		#expect(media.state.allowed)
		queries.consent = .denied; queries.mediaHook = { await gate.wait(); return [] }
		media.send(.load); #expect(!media.state.allowed)
		for _ in 0 ..< 10000 {
			if gate.started {
				break
			}; await Task.yield()
		}
		try #require(gate.started); #expect(!media.state.allowed)
		gate.release(); await media.close()
	}
}
