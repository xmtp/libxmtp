@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class CommitHandleAccount: HandleAccount {
	let minimum = 3
	var claimTimes: [String: Date] = [:]
	var claims = 0
	func handles() async throws -> OwnHandles {
		OwnHandles(current: nil)
	}

	func availableHandles(name: String) async throws -> [String] {
		[name]
	}

	func claimHandle(handle _: String) async throws {
		claims += 1
	}

	func recordClaim(handle: String, at date: Date) throws {
		claimTimes[handle] = date
	}
}

struct HandleInputCommitTests {
	@Test(arguments: ["abc", "ABC"]) @MainActor
	func semanticNoOpCommitPreservesAvailableClaim(input: String) async throws {
		let account = CommitHandleAccount()
		let store = HandleStore(account: account, mode: .change,
		                        origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		                        admit: { _ in true }, finish: {})
		store.send(.load); store.send(.changeInput("abc")); store.send(.check)
		for _ in 0 ..< 10000 {
			if store.state.canClaim {
				break
			}; await Task.yield()
		}
		try #require(store.state.canClaim)
		let available = store.state
		store.send(.changeInput(input))
		#expect(store.state == available)
		store.send(.claim)
		#expect(store.state.confirm == "abc"); #expect(account.claims == 0)
		store.send(.changeInput("abcd"))
		#expect(store.state.input == "abcd"); #expect(store.state.confirm == nil); #expect(!store.state.canClaim)
		await store.close()
	}
}
