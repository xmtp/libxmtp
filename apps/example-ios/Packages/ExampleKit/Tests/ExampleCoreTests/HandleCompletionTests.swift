@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class CompletionAccount: HandleAccount {
	let minimum = 3
	var claimTimes: [String: Date] = [:]
	var current: String?
	var claims: [String] = []
	var skipFails = true
	func handles() async throws -> OwnHandles {
		OwnHandles(current: current)
	}

	func availableHandles(name: String) async throws -> [String] {
		[name]
	}

	func claimHandle(handle: String) async throws {
		claims.append(handle); current = handle
	}

	func recordClaim(handle: String, at date: Date) throws {
		claimTimes[handle] = date
	}
}

struct HandleCompletionTests {
	@Test @MainActor func successfulChooseClaimBypassesFailingSkipPreference() async throws {
		let account = CompletionAccount(); let routes = RouteOwner(); routes.replace(.chooseHandle)
		var skipWrites = 0
		let store = HandleStore(account: account, mode: .choose, origin: origin, admit: { _ in true },
		                        finish: { routes.replace(.chats) },
		                        persistSkip: { skipWrites += 1; throw HandleError.failed })
		store.send(.load); try await wait { !store.state.loading }
		store.send(.changeInput("claimme")); store.send(.check)
		try await wait { store.state.canClaim }
		store.send(.claim); try await wait { !account.claimTimes.isEmpty }
		#expect(account.claims == ["claimme"]); #expect(account.claimTimes["claimme"] != nil)
		#expect(skipWrites == 0); #expect(routes.current == .chats); #expect(store.state.notice == nil)
		await store.close()
	}

	@Test @MainActor func existingHandleFinishesWithoutSavingSkip() async throws {
		let account = CompletionAccount(); account.current = "alreadyclaimed"
		let routes = RouteOwner(); routes.replace(.chooseHandle); var skipWrites = 0
		let store = HandleStore(account: account, mode: .choose, origin: origin, admit: { _ in true },
		                        finish: { routes.replace(.chats) },
		                        persistSkip: { skipWrites += 1; throw HandleError.failed })
		store.send(.load); try await wait { !store.state.loading }
		#expect(skipWrites == 0); #expect(routes.current == .chats); #expect(account.claims.isEmpty)
		await store.close()
	}

	@Test @MainActor func explicitSkipFailureRetriesPreferenceWithoutClaiming() async {
		let account = CompletionAccount(); let routes = RouteOwner(); routes.replace(.chooseHandle)
		var skipWrites = 0
		let store = HandleStore(account: account, mode: .choose, origin: origin, admit: { _ in true },
		                        finish: { routes.replace(.chats) }, persistSkip: {
		                        	skipWrites += 1; #expect(routes.current == .chooseHandle)
		                        	if account.skipFails {
		                        		throw HandleError.failed
		                        	}
		                        })
		store.send(.skip)
		#expect(skipWrites == 1); #expect(store.state.notice == .skipFailed); #expect(routes.current == .chooseHandle)
		#expect(HandleCopy.notice(.skipFailed) == "Cannot save your preference. Try again.")
		account.skipFails = false; store.send(.retry)
		#expect(skipWrites == 2); #expect(store.state.notice == nil); #expect(routes.current == .chats)
		#expect(account.claims.isEmpty); #expect(account.claimTimes.isEmpty)
		await store.close()
	}

	@Test(arguments: [false, true]) @MainActor func staleSkipDoesNotSaveOrNavigate(sessionReplaced: Bool) async {
		let account = CompletionAccount(); var writes = 0; var finished = false
		let store = HandleStore(account: account, mode: .choose, origin: origin, admit: { _ in sessionReplaced },
		                        finish: { finished = true }, sessionValid: { _ in !sessionReplaced },
		                        persistSkip: { writes += 1 })
		store.send(.skip); store.send(.retry)
		#expect(writes == 0); #expect(!finished); #expect(store.state.notice == nil)
		await store.close()
	}

	@MainActor private var origin: DestinationOrigin {
		DestinationOrigin(
			session: SessionKey(profileID: UUID(), generation: 1),
			token: 0,
		)
	}

	@MainActor private func wait(_ ready: () -> Bool) async throws {
		let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(2))
		while !ready(), clock.now < deadline {
			await Task.yield()
		}
		try #require(ready())
	}
}
