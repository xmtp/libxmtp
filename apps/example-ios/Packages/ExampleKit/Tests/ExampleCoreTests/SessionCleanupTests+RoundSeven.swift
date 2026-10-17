@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class RoundSevenSecrets: SecretStore {
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

@MainActor private final class RoundSevenFiles: ProfileFileStore {
	var index = SessionIndex()
	var removed: [String] = []
	var hasDatabase = false
	var failLoad = false
	var failSave = false
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
		if failSave {
			throw SessionError.message("Cannot save profile.")
		}
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

@MainActor private final class RoundSevenClient: SessionClient {
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

@MainActor private final class RoundSevenFactory: SessionFactory {
	let client = RoundSevenClient()
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
			failNext = false; throw SessionError.message("RoundSevenFactory offline")
		}
		defer { finished = true }
		if pause {
			return try await withCheckedThrowingContinuation { pending = $0 }
		}
		return client
	}
}

private struct JoinProbe: BackendProbe {
	func probe(_: BackendURL) async throws -> BackendProbeResult {
		BackendProbeResult(requiresToken: true)
	}
}

extension SessionCleanupTests {
	@Test(arguments: [false, true], [false, true]) @MainActor func failedLifecycleIndexKeepsSessionAdmitted(
		reset: Bool,
		write: Bool,
	) async throws {
		let files = RoundSevenFiles(); let secrets = RoundSevenSecrets(); let factory = RoundSevenFactory(); let fence =
			SessionFence()
		let session = SessionController(fence: fence, secrets: secrets, files: files, factory: factory)
		try await session.connect(url: "http://localhost:5050", token: "fixture-token")
		let generation = fence.currentGeneration(); let state = session.state
		files.failLoad = !write; files.failSave = write
		var failed = false
		do {
			if reset {
				try await session.reset()
			} else {
				try await session.signOut()
			}
		} catch { failed = true }
		#expect(failed); #expect(fence.currentGeneration() == generation); #expect(session.state == state); #expect(!factory
			.client.ended)
	}

	@Test @MainActor func successfulJoinClearsUIBearerToken() async throws {
		let files = RoundSevenFiles(); let secrets = RoundSevenSecrets(); let factory = RoundSevenFactory()
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		let store = ConnectStore(
			probe: JoinProbe(),
			session: session,
			owner: RouteOwner(),
			defaultURL: "http://localhost:5050",
		)
		store.send(.appear)
		let probeClock = ContinuousClock(); let probeDeadline = probeClock.now.advanced(by: .seconds(3))
		while !store.state.ready, probeClock.now < probeDeadline {
			await Task.yield()
		}
		try #require(store.state.ready)
		store.send(.tokenChanged("fixture-token")); store.send(.join)
		while !factory.finished {
			await Task.yield()
		}; await Task.yield()
		#expect(store.state.token.isEmpty); #expect(secrets.values["credential"] != nil)
	}
}

extension SessionCleanupTests {
	@Test @MainActor func changedServerClearsSupersededJoiningState() async throws {
		let files = RoundSevenFiles(); let secrets = RoundSevenSecrets(); let factory = RoundSevenFactory(); factory
			.pause = true
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		let store = ConnectStore(
			probe: JoinProbe(),
			session: session,
			owner: RouteOwner(),
			defaultURL: "http://localhost:5050",
		)
		store.send(.appear)
		let probeClock = ContinuousClock(); let probeDeadline = probeClock.now.advanced(by: .seconds(3))
		while !store.state.ready, probeClock.now < probeDeadline {
			await Task.yield()
		}
		try #require(store.state.ready)
		store.send(.tokenChanged("fixture-token")); store.send(.join)
		for _ in 0 ..< 10000 {
			if factory.pending != nil {
				break
			}; await Task.yield()
		}
		try #require(factory.pending != nil)
		defer {
			factory.pending?.resume(returning: factory.client)
			factory.pending = nil
		}
		store.send(.addressChanged("http://localhost:5051"))
		#expect(!store.state.joining)
		let clock = ContinuousClock()
		let deadline = clock.now.advanced(by: .seconds(3))
		while store.state.checking, clock.now < deadline {
			await Task.yield()
		}
		#expect(store.state.ready); #expect(!store.state.checking)
		store.send(.tokenChanged("new-fixture-token")); #expect(store.state.canJoin)
	}
}

extension SessionCleanupTests {
	@Test @MainActor func resetRetryIndexReadFailureKeepsLiveSessionAdmitted() async throws {
		let files = RoundSevenFiles(); let factory = RoundSevenFactory(); let fence = SessionFence()
		let session = SessionController(fence: fence, secrets: RoundSevenSecrets(), files: files, factory: factory)
		try await session.connect(url: "http://localhost:5050", token: "fixture-token")
		let info: SessionInfo
		if case let .active(active) = session.state {
			info = active
		} else {
			Issue.record("Expected active session"); return
		}
		files.index.reset = ResetJournal(profileID: info.profile.id, phase: .stopping)
		files.failLoad = true
		do {
			try await session.continueReset(); Issue.record("Expected index read failure")
		} catch {
			#expect((error as? SessionError) == .message("Cannot read profile."))
		}
		#expect(fence.accepts(info.key)); #expect(session.state == .active(info))
		#expect(session.client != nil); #expect(!factory.client.ended); #expect(files.removed.isEmpty)
		files.failLoad = false
		try await session.continueReset()
		#expect(session.state == .signedOut); #expect(factory.client.ended); #expect(files.index.reset == nil)
	}
}
