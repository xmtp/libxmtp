import ExampleCore
import ExampleSDK
import SwiftUI

extension AppGraph {
	func navigate(_ route: Route) {
		routes.show(route)
		if route == .newConversation {
			prepareNewConversation()
		}
		if route == .changeHandle {
			prepareHandle(.change)
		}
	}

	func prepareHandle(_ mode: HandleMode) {
		guard let info = active, let client = factory.client(for: info.key), let preferences else { return }
		let origin = DestinationOrigin(session: info.key, token: routes.token)
		handles = HandleStore(
			account: SDKHandleAccount(client: client, limiter: reads, claimTimes: preferences.claimTimes,
			                          recordClaim: preferences.recordClaim),
			mode: mode,
			origin: origin,
			admit: { [weak self] origin in guard let self else { return false }; return routes.admit(origin, fence: fence) },
			finish: { [weak self] in
				guard let self else { return }
				completeHandleSelection(mode, routes: routes)
				session.scope.add { [weak self] in
					guard let self else { return }
					do {
						let own = try await SDKHandleAccount(client: client, limiter: reads).handles(); guard fence.accepts(info.key)
						else { return }; currentHandle = own.current; otherHandles = own.others
					} catch {}
				}
			},
			sessionValid: { [weak self] key in self?.fence.accepts(key) == true },
			persistSkip: preferences.skipHandle,
		)
		if let handles {
			let scope = session.scope
			scope.setCleanup(for: "handle", with: { await handles.close() })
		}
	}

	func prepareNewConversation() {
		guard let info = active, let client = factory.client(for: info.key) else { return }
		let identities = SDKIdentityQueries(client: client, limiter: reads)
		newConversation = NewConversationStore(
			queries: SDKNewConversationQueries(client: client, identity: identities, reads: reads),
			origin: DestinationOrigin(session: info.key, token: routes.token),
			admit: { [weak self] origin in guard let self else { return false }; return routes.admit(origin, fence: fence) },
			finish: { [weak self] id in self?.routes.back(); self?.routes.show(.chat(id)) },
		)
		if let newConversation {
			let scope = session.scope
			scope.setCleanup(for: "new-conversation", with: { await newConversation.close() })
		}
	}

	func closeInactiveDestinations() async {
		let scope = session.scope
		if routes.current != .chooseHandle, routes.current != .changeHandle {
			handles = nil
			await scope.runCleanup(for: "handle")
		}
		if routes.current != .newConversation {
			newConversation = nil
			await scope.runCleanup(for: "new-conversation")
		}
	}

	func setAppearance(_ value: String) {
		do { var index = try files.loadIndex(); index.appearance = value; try files.saveIndex(index); appearance = value
		} catch { self.error = "Cannot save appearance. Try again." }
	}

	func signOut() async {
		do { try await session.signOut(); await bindSession() } catch { self.error = "Cannot sign out. Try again." }
	}

	func reset() async {
		do { try await session.reset(); await bindSession() } catch { self.error = "Local deletion is incomplete" }
	}
}

@MainActor func completeHandleSelection(
	_ mode: HandleMode,
	routes: RouteOwner,
) {
	if mode == .choose {
		routes.replace(.chats)
	} else {
		routes.back()
	}
}
