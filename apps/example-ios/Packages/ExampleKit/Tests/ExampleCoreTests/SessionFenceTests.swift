@testable import ExampleCore
import Foundation
import Testing

@MainActor @Test func reservationsRejectOldSessions() {
	let fence = SessionFence()
	let id = UUID()
	let first = fence.replace(id)
	#expect(first != nil)
	if let first {
		#expect(fence.accepts(first)); _ = fence
			.reserve(); #expect(!fence.accepts(first)); #expect(fence.withCurrent(first) { true } == nil)
	}
	#expect(fence.reserveRestore() == nil)
}

@MainActor @Test func restoreCannotOverrideExplicitAction() {
	let fence = SessionFence()
	let first = fence.reserveRestore()
	#expect(first != nil)
	let explicit = fence.reserve()
	#expect(fence.reserveRestoreIfCurrent(explicit) == nil)
	if let first {
		#expect(fence.bind(UUID(), reserved: first) == nil)
	}
}

@MainActor @Test func destinationsRejectCompletionAfterNavigation() {
	let fence = SessionFence(); let routes = RouteOwner()
	guard let key = fence.replace(UUID()) else { Issue.record("No session"); return }
	let origin = DestinationOrigin(session: key, token: routes.token)
	#expect(routes.admit(origin, fence: fence))
	routes.show(.settings)
	#expect(!routes.admit(origin, fence: fence))
}

@MainActor @Test func restoreCannotReserveOverBoundProfile() {
	let fence = SessionFence()
	guard let generation = fence.reserveRestore(),
	      let key = fence.bind(UUID(), reserved: generation) else { Issue.record("Could not bind restore"); return }
	#expect(fence.reserveRestoreIfCurrent(generation) == nil)
	#expect(fence.accepts(key))
}

@MainActor @Test func directRestoreCannotReplaceBoundProfile() {
	let fence = SessionFence()
	guard let generation = fence.reserveRestore(),
	      let key = fence.bind(UUID(), reserved: generation)
	else { Issue.record("Could not bind restored profile"); return }
	#expect(fence.reserveRestore() == nil)
	#expect(fence.accepts(key))
}

@Test @MainActor func delayedSheetDismissDoesNotPopCreatedChat() {
	let routes = RouteOwner()
	routes.replace(.chats)
	routes.show(.newConversation)
	routes.back()
	routes.show(.chat("created"))
	let destinationToken = routes.token
	routes.dismissSheet()
	#expect(routes.current == .chat("created"))
	#expect(routes.token == destinationToken)
	routes.show(.newConversation)
	routes.dismissSheet()
	#expect(routes.sheet == nil)
	#expect(routes.current == .chat("created"))
}

@Test @MainActor func samePathUpdateKeepsPresentedDestination() {
	let fence = SessionFence()
	let routes = RouteOwner()
	guard let key = fence.replace(UUID()) else { Issue.record("No session"); return }
	routes.replace(.chats)
	routes.show(.newConversation)
	let origin = DestinationOrigin(session: key, token: routes.token)
	routes.setPath(routes.path)
	#expect(routes.current == .newConversation)
	#expect(routes.sheet == .newConversation)
	#expect(routes.admit(origin, fence: fence))
	routes.setPath([.settings])
	#expect(routes.current == .settings)
	#expect(routes.sheet == nil)
	#expect(!routes.admit(origin, fence: fence))
}
