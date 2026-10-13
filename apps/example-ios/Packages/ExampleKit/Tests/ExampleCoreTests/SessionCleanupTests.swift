@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class Secrets: SecretStore {
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

@MainActor private final class Files: ProfileFileStore {
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

@MainActor private final class Client: SessionClient {
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

@MainActor private final class Factory: SessionFactory {
	let client = Client()
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
			failNext = false; throw SessionError.message("Factory offline")
		}
		defer { finished = true }
		if pause {
			return try await withCheckedThrowingContinuation { pending = $0 }
		}
		return client
	}
}

struct SessionCleanupTests {
	@Test @MainActor func signOutKeepsLocalIdentity() async throws {
		let files = Files(); let secrets = Secrets(); let factory = Factory()
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		try await session.connect(url: "http://localhost:5050", token: "token")
		session.scope.addCleanup { factory.client.cleaned = true }
		try await session.signOut()
		#expect(factory.client.ended)
		#expect(factory.client.endedAfterCleanup)
		#expect(secrets.values["credential"] == nil)
		#expect(secrets.values["wallet"] != nil)
		#expect(secrets.values["database-key"] != nil)
		#expect(files.removed == ["exports"])
		#expect(!files.index.signedIn)
	}

	@Test(arguments: [ResetPhase.stopping, .databaseRemoved, .filesRemoved]) @MainActor
	func resumesReset(phase: ResetPhase) async throws {
		let files = Files(); let secrets = Secrets(); let profile = BackendProfile(url: "https://example.com")
		files.index.profiles = [profile]; files.index.activeID = profile.id
		files.index.reset = ResetJournal(profileID: profile.id, phase: phase)
		secrets.values["wallet"] = Data([1])
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: Factory())
		await session.restore(); #expect(session.state == .resetIncomplete)
		try await session.continueReset()
		#expect(files.index.reset == nil); #expect(files.index.profiles.isEmpty); #expect(secrets.values.isEmpty)
		#expect(files.removed.contains("database") == (phase == .stopping))
		#expect(files.removed.contains("profile") == (phase != .filesRemoved))
	}

	@Test func symlinkNeverDeletesItsTarget() throws {
		let manager = FileManager.default
		let temp = manager.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		let root = temp.appendingPathComponent("profile")
		let outside = temp.appendingPathComponent("outside")
		try manager.createDirectory(at: root, withIntermediateDirectories: true)
		try manager.createDirectory(at: outside, withIntermediateDirectories: true)
		defer { do { try manager.removeItem(at: temp) } catch {} }
		let file = outside.appendingPathComponent("keep")
		try Data([1]).write(to: file)
		let link = root.appendingPathComponent("exports")
		try manager.createSymbolicLink(at: link, withDestinationURL: outside)
		try ProfileCleanup.remove(link, inside: root)
		#expect(manager.fileExists(atPath: file.path))
		#expect(throws: (any Error).self) { try ProfileCleanup.remove(outside, inside: root) }
	}

	@Test(arguments: ["wallet", "database-key", "preferences-key"]) @MainActor
	func restoreDoesNotReplaceMissingSecrets(missing: String) async {
		let files = Files(); let secrets = Secrets(); let factory = Factory()
		let profile = BackendProfile(url: "https://example.com", inboxID: "saved", identity: "identity")
		files.index.profiles = [profile]; files.index.activeID = profile.id; files.index.signedIn = true; files
			.hasDatabase = true
		for ref in ["wallet", "database-key", "preferences-key"] where ref != missing {
			secrets.values[ref] = Data(repeating: 7, count: 32)
		}
		let before = secrets.values
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		await session.restore()
		#expect(session.state == .restoreFailed)
		#expect(secrets.values == before)
		#expect(factory.opens == 0)
	}

	@Test @MainActor func joinFailureBeforeReservationAllowsRetry() async throws {
		let files = Files(); let fence = SessionFence()
		let session = SessionController(fence: fence, secrets: Secrets(), files: files, factory: Factory())
		let store = ConnectStore(
			probe: ReadyProbe(),
			session: session,
			owner: RouteOwner(),
			defaultURL: "https://example.com",
		)
		store.send(.appear)
		try await waitUntil { !store.state.checking }
		files.failLoad = true
		store.send(.join)
		try await waitUntil { !store.state.joining }
		#expect(store.state.error == "Cannot read profile.")
		#expect(store.state.canJoin)
		#expect(fence.currentGeneration() == 0)
		files.failLoad = false
		store.send(.join)
		try await waitUntil { !store.state.joining }
		#expect(store.state.error == nil)
		#expect(session.client != nil)
	}

	@Test(arguments: [false, true]) @MainActor
	func delayedJoinFailureCannotChangeNewOwner(changeSession: Bool) async throws {
		let factory = Factory(); factory.pause = true
		let fence = SessionFence(); let owner = RouteOwner()
		let session = SessionController(fence: fence, secrets: Secrets(), files: Files(), factory: factory)
		let store = ConnectStore(probe: ReadyProbe(), session: session, owner: owner, defaultURL: "https://example.com")
		try await waitUntil { !store.state.checking }
		store.send(.join)
		try await waitUntil { factory.pending != nil }
		if changeSession {
			_ = fence.reserve()
		} else {
			owner.show(.settings)
		}
		let state = store.state
		factory.pending?.resume(throwing: SessionError.message("Late failure")); factory.pending = nil
		try await waitUntil { factory.finished }
		try await Task.sleep(for: .milliseconds(10))
		#expect(store.state == state)
	}

	@Test @MainActor func failedConnectRecoveryRetriesWithExplicitReservation() async throws {
		let files = Files(); let secrets = Secrets(); let factory = Factory()
		let profile = BackendProfile(url: "https://example.com", inboxID: "saved", identity: "identity")
		files.index.profiles = [profile]; files.hasDatabase = true
		secrets.values["database-key"] = Data(repeating: 7, count: 32)
		secrets.values["preferences-key"] = Data(repeating: 7, count: 32)
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		do { try await session.connect(url: profile.url, token: "") } catch {
			#expect((error as? SessionError) == .missingSecret)
		}
		#expect(session.state == .restoreFailed)
		#expect(!files.index.signedIn)
		secrets.values["wallet"] = Data(repeating: 7, count: 32)
		let store = ConnectStore(
			probe: ReadyProbe(),
			session: session,
			owner: RouteOwner(),
			defaultURL: "https://example.com",
		)
		store.send(.retryRestore)
		try await waitUntil { session.client != nil }
		#expect(files.index.signedIn)
		#expect(files.index.activeID == profile.id)
		#expect(factory.opens == 1)
	}

	@Test @MainActor func indexReadFailureEndsUncommittedClient() async {
		let files = Files(); files.failOnLoad = 2
		let factory = Factory()
		let session = SessionController(fence: SessionFence(), secrets: Secrets(), files: files, factory: factory)
		do {
			try await session.connect(url: "https://example.com", token: "")
			Issue.record("Expected index read failure")
		} catch { #expect((error as? SessionError) == .message("Cannot read profile.")) }
		#expect(factory.opens == 1)
		#expect(factory.client.ended)
		#expect(session.client == nil)
		#expect(!files.index.signedIn)
	}

	@Test @MainActor func incompleteProfileRetriesWithSameWallet() async throws {
		let files = Files(); let secrets = Secrets(); let factory = Factory(); factory.failNext = true
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		do {
			try await session.connect(url: "https://example.com", token: "")
			Issue.record("Expected first creation to fail")
		} catch { #expect((error as? SessionError) == .message("Factory offline")) }
		let saved = try #require(files.index.profiles.first)
		let savedSecrets = secrets.values
		try await session.connect(url: "https://example.com", token: "")
		#expect(factory.profiles == [saved.id, saved.id])
		#expect(factory.wallets.count == 2)
		#expect(factory.wallets.first == factory.wallets.last)
		#expect(secrets.values == savedSecrets)
		#expect(files.index.profiles.count == 1)
		#expect(files.index.signedIn)
		#expect(session.client != nil)
	}

	@Test @MainActor func keyedRouteCleanupReleasesEveryVisit() async {
		let scope = SessionScope(); let events = CleanupEvents()
		func install(_ name: String) async -> WeakRouteResource {
			let resource = RouteResource(name: name, events: events)
			let weak = WeakRouteResource(resource)
			await scope.replaceCleanup(for: "route") { await resource.close() }
			return weak
		}
		let first = await install("first")
		let second = await install("second")
		#expect(first.value == nil)
		#expect(events.closed == ["first"])
		await scope.runCleanup(for: "route")
		await scope.runCleanup(for: "route")
		#expect(second.value == nil)
		#expect(events.closed == ["first", "second"])
		for name in ["third", "fourth", "fifth"] {
			let visit = await install(name)
			await scope.runCleanup(for: "route")
			#expect(visit.value == nil)
		}
		let pending = await install("pending")
		scope.addCleanup { events.closed.append("session") }
		await scope.cancelAndWait()
		#expect(pending.value == nil)
		#expect(events.closed == ["first", "second", "third", "fourth", "fifth", "session", "pending"])
	}

	@Test(arguments: ["credential", "exports", "both"]) @MainActor
	func signOutCleanupFailureStillEndsSession(failure: String) async throws {
		let files = Files(); let secrets = Secrets(); let factory = Factory()
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		try await session.connect(url: "https://example.com", token: "token")
		secrets.failCredentialRemoval = failure != "exports"
		files.failExports = failure != "credential"
		do {
			try await session.signOut()
			Issue.record("Expected cleanup failure")
		} catch {
			let expected = failure == "exports" ? "Export cleanup failed" : "Credential cleanup failed"
			#expect((error as? SessionError) == .message(expected))
		}
		#expect(session.state == .signedOut)
		#expect(session.client == nil)
		#expect(factory.client.ended)
		#expect(!files.index.signedIn)
		#expect(secrets.values["wallet"] != nil)
		#expect(secrets.credentialRemovalAttempts == 1)
		#expect(files.exportRemovalAttempts == 1)
		#expect((secrets.values["credential"] == nil) == !secrets.failCredentialRemoval)
		#expect(files.removed.contains("exports") == !files.failExports)
	}

	@Test(arguments: [SettingsConfirmation.reset, .signOut]) @MainActor
	func settingsDismissalOnlyClearsMatchingConfirmation(choice: SettingsConfirmation) {
		let store = SettingsStore(appearance: "system", valid: { true }, saveAppearance: { _ in }, signOut: {}, reset: {})
		store.send(choice == .reset ? .reset : .signOut)
		let other: SettingsConfirmation = choice == .reset ? .signOut : .reset
		store.dismissConfirmation(other)
		#expect(store.state.confirmation == choice)
		store.dismissConfirmation(choice)
		#expect(store.state.confirmation == nil)
	}

	@MainActor private func waitUntil(_ condition: () -> Bool) async throws {
		let deadline = ContinuousClock.now.advanced(by: .seconds(2))
		while !condition(), ContinuousClock.now < deadline {
			try await Task.sleep(for: .milliseconds(5))
		}
		#expect(condition())
	}
}

private struct ReadyProbe: BackendProbe {
	func probe(_: BackendURL) async throws -> BackendProbeResult {
		BackendProbeResult(requiresToken: false)
	}
}

@MainActor private final class CleanupEvents { var closed: [String] = [] }
@MainActor private final class RouteResource {
	let name: String
	let events: CleanupEvents
	init(name: String, events: CleanupEvents) {
		self.name = name; self.events = events
	}

	func close() async {
		events.closed.append(name)
	}
}

@MainActor private final class WeakRouteResource {
	weak var value: RouteResource?
	init(_ value: RouteResource) {
		self.value = value
	}
}
