@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class ProbeSecrets: SecretStore {
	var values: [String: Data] = [:]
	var failCredentialRemoval = false
	var credentialRemovalAttempts = 0
	func read(_: UUID, ref: String) throws -> Data? {
		values[ref]
	}

	func write(_: UUID, ref: String, data: Data) throws {
		values[ref] = data
	}

	func remove(_: UUID, ref: String) throws {
		if ref == "credential" {
			credentialRemovalAttempts += 1
		}
		if failCredentialRemoval, ref == "credential" {
			throw SessionError.message("Credential cleanup failed")
		}
		values[ref] = nil
	}

	func removeAll(_: UUID) throws {
		values = [:]
	}

	func randomBytes(count: Int) throws -> Data {
		Data(repeating: 1, count: count)
	}
}

@MainActor private final class ProbeFiles: ProfileFileStore {
	var index = SessionIndex()
	var removed: [String] = []
	var hasDatabase = false
	var failLoad = false
	var failExports = false
	var exportRemovalAttempts = 0
	var failOnLoad: Int?
	var loads = 0
	func loadIndex() throws -> SessionIndex {
		loads += 1
		if failLoad || loads == failOnLoad {
			throw SessionError.message("Cannot read profile.")
		}; return index
	}

	func saveIndex(_ index: SessionIndex) throws {
		self.index = index
	}

	func paths(_ profile: UUID) throws -> ProfilePaths {
		ProfilePaths(root: URL(fileURLWithPath: "/tmp/" + profile.uuidString))
	}

	func databaseExists(_: UUID) throws -> Bool {
		hasDatabase
	}

	func removeDatabase(_: UUID) throws {
		removed.append("database")
	}

	func removeExports(_: UUID) throws {
		exportRemovalAttempts += 1
		if failExports {
			throw SessionError.message("Export cleanup failed")
		}
		removed.append("exports")
	}

	func removeProfile(_: UUID) throws {
		removed.append("profile")
	}
}

@MainActor private final class ProbeClient: SessionClient {
	let inboxID = "inbox"
	let identity = "identity"
	var ended = false
	var cleaned = false
	var endedAfterCleanup = false
	func end() async throws {
		ended = true; endedAfterCleanup = cleaned
	}

	func disableNotifications() async throws {}
}

@MainActor private final class ProbeFactory: SessionFactory {
	let client = ProbeClient()
	var opens = 0
	var failNext = false
	var profiles: [UUID] = []
	var wallets: [Data] = []
	var pause = false
	var finished = false
	var pending: CheckedContinuation<any SessionClient, any Error>?
	// Matches the explicit security inputs of SessionFactory.
	// swiftlint:disable:next function_parameter_count
	func open(
		profile: BackendProfile,
		paths _: ProfilePaths,
		wallet: Data,
		databaseKey _: Data,
		secrets _: any SecretStore,
		key _: SessionKey,
		fence _: SessionFence,
	) async throws -> any SessionClient {
		opens += 1; profiles.append(profile.id); wallets.append(wallet)
		if failNext {
			failNext = false; throw SessionError.message("ProbeFactory offline")
		}
		defer { finished = true }
		if pause {
			return try await withCheckedThrowingContinuation { pending = $0 }
		}
		return client
	}
}

private actor RecoveringProbe: BackendProbe {
	var calls = 0
	func probe(_: BackendURL) async throws -> BackendProbeResult {
		calls += 1
		if calls == 1 {
			throw SessionError.message("Probe unavailable")
		}
		return BackendProbeResult(requiresToken: true)
	}
}

struct ConnectProbeRecoveryTests {
	@Test func joinRequiresKnownServerConfiguration() {
		var state = ConnectState(address: "https://example.com")
		#expect(!state.canJoin)
		state.error = "Cannot reach this server. Try again."
		#expect(!state.canJoin)
		state.error = nil; state.ready = true
		#expect(state.canJoin)
		state.error = "Access token was not accepted. Check it and try again."
		#expect(state.canJoin)
	}

	@Test @MainActor func failedProbeCannotJoinAndRetryRevealsRequiredToken() async throws {
		let factory = ProbeFactory()
		let session = SessionController(fence: SessionFence(), secrets: ProbeSecrets(), files: ProbeFiles(), factory: factory)
		let store = ConnectStore(
			probe: RecoveringProbe(),
			session: session,
			owner: RouteOwner(),
			defaultURL: "https://example.com",
		)
		store.send(.appear)
		try await settled(store)
		#expect(store.state.error == "Cannot reach this server. Try again.")
		#expect(!store.state.ready); #expect(!store.state.requiresToken); #expect(!store.state.canJoin)
		store.send(.join)
		#expect(!store.state.joining)
		for _ in 0 ..< 20 {
			await Task.yield()
		}
		#expect(factory.opens == 0)
		store.send(.retryProbe)
		#expect(store.state.checking); #expect(!store.state.canJoin)
		try await settled(store)
		#expect(store.state.error == nil); #expect(store.state.ready); #expect(store.state.requiresToken)
		#expect(!store.state.canJoin)
		store.send(.tokenChanged("fixture-token"))
		#expect(store.state.canJoin)
	}

	@Test(arguments: ["https://example.com/", "https://other.example.com"]) @MainActor
	func repeatedAddressCommitDoesNotCancelQueuedJoin(changedAddress: String) async throws {
		let factory = ProbeFactory(); factory.pause = true
		let session = SessionController(fence: SessionFence(), secrets: ProbeSecrets(), files: ProbeFiles(), factory: factory)
		let store = ConnectStore(
			probe: UnprotectedProbe(),
			session: session,
			owner: RouteOwner(),
			defaultURL: "https://example.com",
		)
		store.send(.appear); try await settled(store)
		store.send(.tokenChanged("non-secret-fixture")); store.send(.join)
		store.send(.addressChanged("https://example.com"))
		#expect(store.state.joining); #expect(!store.state.checking); #expect(store.state.ready)
		#expect(store.state.token == "non-secret-fixture")
		let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(1))
		while factory.pending == nil, clock.now < deadline {
			await Task.yield()
		}
		defer { factory.pending?.resume(returning: factory.client); factory.pending = nil }
		try #require(factory.opens == 1)
		try #require(factory.pending != nil)
		store.send(.addressChanged(changedAddress))
		#expect(!store.state.joining); #expect(store.state.checking); #expect(!store.state.ready)
		#expect(store.state.token.isEmpty)
	}

	@MainActor private func settled(_ store: ConnectStore) async throws {
		let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(3))
		while store.state.checking, clock.now < deadline {
			await Task.yield()
		}
		try #require(!store.state.checking)
	}
}

private struct UnprotectedProbe: BackendProbe {
	func probe(_: BackendURL) async throws -> BackendProbeResult {
		BackendProbeResult(requiresToken: false)
	}
}
