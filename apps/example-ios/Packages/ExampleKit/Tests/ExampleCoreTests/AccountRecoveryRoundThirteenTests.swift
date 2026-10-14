@testable import ExampleCore
import Foundation
import Testing

private actor OptionalRequestCountQuery: ConversationQueries {
	var reads = 0
	func list(consent: ConversationConsent, limit _: Int) async throws -> [ConversationRecord] {
		reads += 1
		if reads == 2 {
			throw SessionError.message("Count unavailable")
		}
		return [ConversationRecord(id: "loaded", kind: .group, consent: consent)]
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		record
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

@MainActor private final class OptionalCountPreferences: ConversationPreferences {
	func alias(for _: String) -> String? {
		nil
	}

	func muted(for _: String) -> Bool {
		false
	}

	func marker(for _: String) -> Int64 {
		0
	}

	var dismissedRequests: Set<String> {
		[]
	}

	func dismissRequest(_: String) throws {}
}

struct AccountRecoveryRoundThirteenTests {
	@Test(arguments: [ConversationConsent.allowed, .unknown]) @MainActor
	func failedOptionalCountKeepsPrimaryConversations(consent: ConversationConsent) async throws {
		let fence = SessionFence(); let key = try #require(fence.replace(UUID()))
		let store = ConversationsStore(session: key, fence: fence, routes: RouteOwner(),
		                               queries: OptionalRequestCountQuery(), preferences: OptionalCountPreferences(),
		                               ownInboxID: "own", consent: consent)
		await store.refresh()
		#expect(store.state.rows.map(\.id) == ["loaded"])
		#expect(store.state.requestCount == nil); #expect(store.state.error == nil); #expect(!store.state.loading)
		await store.close()
	}
}
