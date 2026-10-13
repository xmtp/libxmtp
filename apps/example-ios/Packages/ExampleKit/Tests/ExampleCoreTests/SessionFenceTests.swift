import Foundation
import Testing
@testable import ExampleCore

@MainActor @Test func reservationsRejectOldSessions() {
 let fence = SessionFence()
 let id = UUID()
 let first = fence.replace(id)
 #expect(first != nil)
 if let first { #expect(fence.accepts(first)); _ = fence.reserve(); #expect(!fence.accepts(first)); #expect(fence.withCurrent(first) { true } == nil) }
 #expect(fence.reserveRestore() == nil)
}
@MainActor @Test func restoreCannotOverrideExplicitAction() {
 let fence = SessionFence()
 let first = fence.reserveRestore()
 #expect(first != nil)
 let explicit = fence.reserve()
 #expect(fence.reserveRestoreIfCurrent(explicit) == nil)
 if let first { #expect(fence.bind(UUID(), reserved: first) == nil) }
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
 guard let generation = fence.reserveRestore(), let key = fence.bind(UUID(), reserved: generation) else { Issue.record("Could not bind restore"); return }
 #expect(fence.reserveRestoreIfCurrent(generation) == nil)
 #expect(fence.accepts(key))
}
