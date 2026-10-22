@testable import ExampleCore
import Foundation
import Testing

/// The composer writes its draft through on each edit, so a draft survives a
/// kill without a background or submit step.
@MainActor struct ComposerDraftWriteTests {
	@Test func editWritesTheDraftOnce() {
		var writes: [(id: String, draft: ComposerDraft)] = []
		let drafts = ComposerDraftStore(read: { _ in nil }, write: { writes.append(($0, $1)) })
		var records: [SendDraft] = []
		let sends = SendDraftStore(read: { records }, write: { records = $0 })
		let coordinator = SendCoordinator(drafts: sends, queries: ChatQueryFake(), accepts: { true })
		let composer = ComposerStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			drafts: drafts,
			sends: sends,
			coordinator: coordinator,
			admit: { _ in true },
		)
		composer.send(.allow(true))
		#expect(writes.isEmpty)
		composer.send(.edit("x"))
		#expect(writes.map(\.id) == ["chat"])
		#expect(writes.map(\.draft.text) == ["x"])
		#expect(writes.first?.draft.requestID == nil)
	}
}
