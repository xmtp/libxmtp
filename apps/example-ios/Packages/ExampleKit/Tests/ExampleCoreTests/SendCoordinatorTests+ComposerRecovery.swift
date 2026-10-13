@testable import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func reloadedInFlightComposerEnablesAfterSuccess() async {
		let fixture = ComposerRecoveryFixture(); let composer = fixture.open()
		fixture.queries.beforeQueue = { composer.send(.reload) }
		composer.send(.edit("first")); composer.send(.submit); await composer.wait()
		#expect(!composer.state.unknown); #expect(composer.state.enabled)
		composer.send(.edit("second")); #expect(composer.state.draft.text == "second")
	}

	@Test func unmatchedComposerReservationCanBeResubmitted() async {
		let fixture = ComposerRecoveryFixture()
		fixture.reserve()
		let composer = fixture.open()
		#expect(composer.state.enabled)
		#expect(fixture.saved.requestID == nil)
		#expect(fixture.saved.submittedRevision == nil)
		#expect(composer.state.draft.text == "kept")
		#expect(composer.state.draft.replyID == "parent")
		composer.send(.submit)
		await composer.wait()
		#expect(fixture.queries.queues == 1)
		#expect(fixture.saved.text.isEmpty)
	}

	@Test func matchingQueueingReservationRemainsBlocked() async throws {
		let fixture = ComposerRecoveryFixture()
		fixture.reserve()
		let request = try #require(fixture.saved.requestID)
		fixture.records = [SendDraft(id: request, conversationID: "chat")]
		let composer = fixture.open()
		#expect(!composer.state.enabled)
		#expect(composer.state.unknown)
		#expect(fixture.saved.requestID != nil)
		composer.send(.edit("replacement"))
		composer.send(.submit)
		await composer.wait()
		#expect(fixture.saved.text == "kept")
		#expect(fixture.queries.queues == 0)
	}

	@Test func reopenedComposerSupersedesScheduledSubmission() async throws {
		let fixture = ComposerRecoveryFixture()
		let original = fixture.open()
		original.send(.edit("kept"))
		original.send(.submit)
		let originalRequest = try #require(fixture.saved.requestID)
		original.send(.reload)
		#expect(fixture.saved.requestID == originalRequest)
		let reopened = fixture.open()
		#expect(reopened.state.enabled)
		reopened.send(.edit("replacement"))
		reopened.send(.submit)
		let replacementRequest = try #require(fixture.saved.requestID)
		#expect(replacementRequest != originalRequest)
		await original.wait()
		await reopened.wait()
		#expect(fixture.queries.queues == 1)
		#expect(fixture.saved.requestID == nil)
		#expect(fixture.saved.text.isEmpty)
	}
}

@MainActor private final class ComposerRecoveryFixture {
	var saved = ComposerDraft()
	var records: [SendDraft] = []
	let queries = ChatQueryFake()
	lazy var drafts = ComposerDraftStore(read: { _ in self.saved }, write: { _, value in self.saved = value })
	lazy var sends = SendDraftStore(read: { self.records }, write: { self.records = $0 })
	lazy var coordinator = SendCoordinator(drafts: sends, queries: queries, accepts: { true })

	func reserve() {
		saved.edit("kept")
		saved.replyID = "parent"
		saved.requestID = UUID()
		saved.submittedRevision = saved.revision
	}

	func open() -> ComposerStore {
		let composer = ComposerStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			drafts: drafts,
			sends: sends,
			coordinator: coordinator,
			admit: { _ in true },
		)
		composer.send(.allow(true))
		return composer
	}
}
