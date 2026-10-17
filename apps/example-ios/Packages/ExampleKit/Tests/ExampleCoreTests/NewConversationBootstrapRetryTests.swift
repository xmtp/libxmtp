@testable import ExampleCore
import Foundation
import Testing

@MainActor struct NewConversationBootstrapRetryTests {
	@Test(arguments: [false, true]) func bootstrapFailureSurvivesEditingAndCanRetry(failPeople: Bool) async throws {
		let queries = BootstrapRetryQueries(failPeople: failPeople)
		let fence = SessionFence(), routes = RouteOwner()
		let key = try #require(fence.replace(UUID()))
		let origin = DestinationOrigin(session: key, token: routes.token)
		let store = NewConversationStore(queries: queries, origin: origin,
		                                 admit: { routes.admit($0, fence: fence) }, finish: { _ in })
		store.send(.group(true)); store.send(.input("first")); store.send(.load)
		await store.waitForBootstrap()
		#expect(store.state.bootstrapError == "Cannot load people. Try again.")
		#expect(!store.state.loadingPeople)
		#expect(!store.state.canContinue)
		store.send(.input("second"))
		#expect(store.state.bootstrapError != nil)
		queries.fail = false
		store.send(.load)
		#expect(store.state.bootstrapError == nil)
		#expect(store.state.loadingPeople)
		await store.waitForBootstrap()
		#expect(store.state.bootstrapError == nil)
		#expect(store.state.maximumMembers == 10)
		#expect(store.state.canContinue)
		#expect(queries.peopleReads == 2)
		#expect(queries.limitReads == (failPeople ? 1 : 2))
		await store.close()
	}

	@Test func groupCannotContinueBeforeLimitIsKnown() {
		var state = NewConversationState()
		state.group = true; state.input = "person"
		#expect(!state.canContinue)
		state.maximumMembers = 10
		#expect(state.canContinue)
	}
}

@MainActor private final class BootstrapRetryQueries: NewConversationQueries {
	let failPeople: Bool
	var fail = true
	var peopleReads = 0
	var limitReads = 0
	init(failPeople: Bool) {
		self.failPeople = failPeople
	}

	func people() async throws -> [Recipient] {
		peopleReads += 1
		if fail, failPeople {
			throw BootstrapRetryFailure.unavailable
		}
		return []
	}

	func maximumMembers() async throws -> Int {
		limitReads += 1
		if fail {
			throw BootstrapRetryFailure.unavailable
		}
		return 10
	}

	func resolve(_ input: String) async throws -> Recipient {
		Recipient(inboxID: input, handle: nil)
	}

	func create(recipients _: [String], group _: Bool, name _: String, description _: String,
	            adminOnly _: Bool) async throws -> String
	{
		"created"
	}
}

private enum BootstrapRetryFailure: Error { case unavailable }
