@testable import ExampleCore
import Foundation
import Testing

@Test(arguments: [
	false,
	true,
]) @MainActor func staleNewConversationActionsDoNotCallQueries(sessionChanged: Bool) async throws {
	let fence = SessionFence(); let routes = RouteOwner(); let queries = AdmissionConversationQueries()
	let key = try #require(fence.replace(UUID()))
	routes.show(.newConversation)
	let origin = DestinationOrigin(session: key, token: routes.token)
	let store = NewConversationStore(queries: queries, origin: origin,
	                                 admit: { routes.admit($0, fence: fence) }, finish: { _ in })
	store.send(.input("peer"))
	if sessionChanged {
		_ = fence.reserve()
	} else {
		routes.back()
	}
	store.send(.load); store.send(.add); store.send(.next); store.send(.create)
	store.send(.input("changed")); store.send(.name("changed"))
	await store.close()
	#expect(queries.calls.isEmpty)
	#expect(store.state.input == "peer")
	#expect(store.state.name.isEmpty)
}

@Test @MainActor func closedNewConversationActionsDoNotCallQueries() async {
	let queries = AdmissionConversationQueries()
	let origin = DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 0)
	let store = NewConversationStore(queries: queries, origin: origin, admit: { _ in true }, finish: { _ in })
	store.send(.input("peer")); await store.close()
	store.send(.load); store.send(.add); store.send(.next); store.send(.create)
	await store.close()
	#expect(queries.calls.isEmpty)
}

@Test @MainActor func queuedNewConversationLoadChecksAdmissionBeforeQueries() async throws {
	let fence = SessionFence(); let routes = RouteOwner(); let queries = AdmissionConversationQueries()
	let key = try #require(fence.replace(UUID()))
	routes.show(.newConversation)
	let origin = DestinationOrigin(session: key, token: routes.token)
	let store = NewConversationStore(queries: queries, origin: origin,
	                                 admit: { routes.admit($0, fence: fence) }, finish: { _ in })
	store.send(.load); routes.back()
	await store.waitForBootstrap(); await store.close()
	#expect(queries.calls.isEmpty)
}

@Test @MainActor func newConversationLoadStopsAfterAdmissionChanges() async {
	let queries = AdmissionConversationQueries(); var valid = true
	queries.onPeople = { valid = false }
	let origin = DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 0)
	let store = NewConversationStore(queries: queries, origin: origin, admit: { _ in valid }, finish: { _ in })
	store.send(.load); await store.waitForBootstrap(); await store.close()
	#expect(queries.calls == ["people"])
}

@MainActor private final class AdmissionConversationQueries: NewConversationQueries {
	var calls: [String] = []
	var onPeople: () -> Void = {}
	func people() async throws -> [Recipient] {
		calls.append("people"); onPeople(); return []
	}

	func maximumMembers() async throws -> Int {
		calls.append("maximum"); return 250
	}

	func resolve(_ input: String) async throws -> Recipient {
		calls.append("resolve"); return Recipient(inboxID: "peer", handle: input)
	}

	func create(recipients _: [String], group _: Bool, name _: String, description _: String,
	            adminOnly _: Bool) async throws -> String
	{
		calls.append("create"); return "conversation"
	}
}
