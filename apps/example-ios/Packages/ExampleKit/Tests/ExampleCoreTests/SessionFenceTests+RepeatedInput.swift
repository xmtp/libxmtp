import ExampleCore
import Foundation
import Testing

@MainActor private struct RepeatedInputQueries: NewConversationQueries {
	func people() async throws -> [Recipient] {
		[]
	}

	func maximumMembers() async throws -> Int {
		10
	}

	func resolve(_: String) async throws -> Recipient {
		Recipient(inboxID: "peer", handle: "peer")
	}

	func create(recipients _: [String], group _: Bool, name _: String, description _: String,
	            adminOnly _: Bool) async throws -> String
	{
		"chat"
	}
}

@Test @MainActor func unchangedRecipientInputKeepsResolution() async {
	let store = NewConversationStore(
		queries: RepeatedInputQueries(),
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in true },
		finish: { _ in },
	)
	store.send(.input("peer")); store.send(.add); store.send(.input("peer"))
	for _ in 0 ..< 1000 {
		if !store.state.recipients.isEmpty {
			break
		}; await Task.yield()
	}
	#expect(store.state.recipients.map(\.inboxID) == ["peer"]); #expect(store.state.input.isEmpty)
	await store.close()
}
