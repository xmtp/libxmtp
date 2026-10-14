import Foundation
import Observation

@MainActor public protocol SessionClient: AnyObject {
	var inboxID: String { get }
	var identity: String { get }
	func end() async throws
	func disableNotifications() async throws
}

@MainActor public protocol SessionFactory: AnyObject {
	// Session factory protocol keeps identity, storage and admission inputs explicit.
	// swiftlint:disable:next function_parameter_count
	func open(
		profile: BackendProfile,
		paths: ProfilePaths,
		wallet: Data,
		databaseKey: Data,
		secrets: any SecretStore,
		key: SessionKey,
		fence: SessionFence,
	) async throws -> any SessionClient
}

public struct SessionInfo: Equatable, Sendable { public let key: SessionKey; public let profile: BackendProfile }
public enum SessionState: Equatable,
	Sendable { case signedOut, opening, active(SessionInfo), restoreFailed, resetIncomplete }
@MainActor public final class SessionScope {
	private var tasks: [Task<Void, Never>] = []
	private var cleanups: [@MainActor @Sendable () async -> Void] = []
	private var finalCleanups: [@MainActor @Sendable () async -> Void] = []
	private var keyedCleanups: [String: @MainActor @Sendable () async -> Void] = [:]
	public init() {}
	@discardableResult public func add(_ operation: @escaping @MainActor @Sendable () async -> Void) -> Task<Void, Never> {
		let task = Task { await operation() }; tasks.append(task); return task
	}

	public func addCleanup(_ cleanup: @escaping @MainActor @Sendable () async -> Void) {
		cleanups.append(cleanup)
	}

	public func addFinalCleanup(_ cleanup: @escaping @MainActor @Sendable () async -> Void) {
		finalCleanups.append(cleanup)
	}

	@discardableResult public func setCleanup(
		for id: String,
		with cleanup: @escaping @MainActor @Sendable () async -> Void,
	) -> Task<Void, Never>? {
		guard let previous = keyedCleanups.updateValue(cleanup, forKey: id) else { return nil }
		return add { await previous() }
	}

	public func replaceCleanup(for id: String, with cleanup: @escaping @MainActor @Sendable () async -> Void) async {
		await setCleanup(for: id, with: cleanup)?.value
	}

	public func runCleanup(for id: String) async {
		let cleanup = keyedCleanups.removeValue(forKey: id)
		await cleanup?()
	}

	public func cancelAndWait() async {
		let final = finalCleanups; finalCleanups = []
		let pending = tasks; tasks = []; let pendingCleanups = cleanups +
			Array(keyedCleanups.values); cleanups = []; keyedCleanups = [:]; pending
			.forEach { $0.cancel() }; for cleanup in pendingCleanups
		{
			await cleanup()
		}
		for task in pending {
			await task.value
		}
		for cleanup in final {
			await cleanup()
		}
	}
}

@MainActor @Observable public final class SessionController {
	public private(set) var state: SessionState = .signedOut
	public let fence: SessionFence
	public private(set) var client: (any SessionClient)?
	public let scope = SessionScope()
	private var restoring = false
	private var recoveryProfile: BackendProfile?
	private let secrets: any SecretStore
	private let files: any ProfileFileStore
	private let factory: any SessionFactory
	public init(fence: SessionFence, secrets: any SecretStore, files: any ProfileFileStore, factory: any SessionFactory) {
		self.fence = fence; self.secrets = secrets; self.files = files; self.factory = factory
	}

	public func restore() async {
		if case .active = state {
			return
		}
		guard !restoring else { return }
		restoring = true; defer { restoring = false }
		var expected = fence.currentGeneration()
		do {
			let index = try files.loadIndex()
			if index.reset != nil {
				state = .resetIncomplete; return
			}
			guard index.signedIn else { return }
			guard let profile = index.profiles.first(where: { $0.id == index.activeID }) else { state = .restoreFailed; return }
			recoveryProfile = profile
			guard try files.databaseExists(profile.id) else { state = .restoreFailed; return }
			guard let generation = fence.reserveRestore() else { return }
			expected = generation
			try await open(profile, generation: generation, token: nil, createSecrets: false)
		} catch { apply(expected) { $0 = .restoreFailed } }
	}

	/// Retry is an explicit user action; automatic-restore precedence must not block it.
	public func retryRestore() async {
		guard !restoring else { return }
		let generation = fence.reserve()
		restoring = true; defer { restoring = false }
		do {
			let index = try files.loadIndex()
			guard index.reset == nil else { apply(generation) { $0 = .resetIncomplete }; return }
			guard let profile = recoveryProfile ?? index.profiles.first(where: { $0.id == index.activeID }),
			      profile.inboxID != nil, profile.identity != nil, try files.databaseExists(profile.id)
			else {
				apply(generation) { $0 = .restoreFailed }; return
			}
			state = .opening
			await close()
			try await open(profile, generation: generation, token: nil, createSecrets: false)
		} catch { apply(generation) { $0 = .restoreFailed } }
	}

	// Restores existing local state only. UI collectors must wait until foreground promotion.

	public func connect(
		url: String,
		token: String,
		name: String? = nil,
		onReserved: (@MainActor (UInt64) -> Void)? = nil,
	) async throws {
		let backend = try BackendURL(url)
		var index = try files.loadIndex()
		guard index.reset == nil else { throw SessionError.message("Finish local reset before connecting") }
		let existing = index.profiles.first { $0.url == backend.value }
		var profile = existing ?? BackendProfile(url: backend.value)
		profile.name = name
		let generation = fence.reserve(); onReserved?(generation); state = .opening
		var missingDatabase = false
		do {
			index.signedIn = false
			if existing == nil {
				index.profiles.append(profile)
			}
			index.activeID = profile.id; try files.saveIndex(index)
			await close()
			let established = profile.inboxID != nil || profile.identity != nil
			let hasDatabase = try files.databaseExists(profile.id)
			if established, !hasDatabase {
				missingDatabase = true; throw SessionError.message("Cannot restore this account. Try again.")
			}
			try await open(profile, generation: generation, token: token, createSecrets: !established && !hasDatabase)
		} catch {
			let recoverableProfile = missingDatabase || (error as? SessionError) == .missingSecret
			if fence.currentGeneration() == generation, recoverableProfile {
				recoveryProfile = profile
			}
			apply(generation) { $0 = recoverableProfile ? .restoreFailed : .signedOut }
			throw error
		}
	}

	private func open(_ profile: BackendProfile, generation: UInt64, token: String?, createSecrets: Bool) async throws {
		guard let key = fence.bind(profile.id, reserved: generation) else { throw SessionError.stale }
		state = .opening
		if let token {
			if token.isEmpty {
				try secrets.remove(profile.id, ref: "credential")
			} else {
				try secrets.write(profile.id, ref: "credential", data: Data(token.utf8))
			}
		}
		func secret(_ ref: String) throws -> Data {
			if let saved = try secrets.read(profile.id, ref: ref) {
				guard saved.count == 32 else { throw SessionError.missingSecret }; return saved
			}; guard createSecrets
			else { throw SessionError.missingSecret }; let data = try secrets.randomBytes(count: 32); try secrets.write(
				profile.id,
				ref: ref,
				data: data,
			); return data
		}
		let wallet = try secret("wallet"); let databaseKey = try secret("database-key"); _ = try secret("preferences-key")
		let opened = try await factory.open(
			profile: profile,
			paths: files.paths(profile.id),
			wallet: wallet,
			databaseKey: databaseKey,
			secrets: secrets,
			key: key,
			fence: fence,
		)
		guard fence.accepts(key) else { try await opened.end(); throw SessionError.stale }
		var saved = profile; saved.inboxID = opened.inboxID; saved.identity = opened.identity
		do {
			var index = try files.loadIndex()
			index.profiles.removeAll { $0.id == saved.id }
			index.profiles.append(saved); index.activeID = saved.id; index.signedIn = true
			try files.saveIndex(index)
		} catch { try await opened.end(); throw error }
		client = opened; recoveryProfile = nil; apply(generation) { $0 = .active(SessionInfo(key: key, profile: saved)) }
	}

	private func close() async {
		let old = client; client = nil
		await scope.cancelAndWait()
		do { try await old?.disableNotifications() } catch { /* Local disable happens before the network call. */ }
		do { try await old?.end() } catch { /* Closing is best effort; local cleanup must continue. */ }
	}

	public func signOut() async throws {
		var index = try files.loadIndex(); index.signedIn = false; try files
			.saveIndex(index)
		let generation = fence.reserve()
		await close()
		guard fence.currentGeneration() == generation else { return }
		defer { apply(generation) { $0 = .signedOut } }
		if let id = index.activeID {
			var cleanupError: (any Error)?
			do { try secrets.remove(id, ref: "credential") } catch { cleanupError = error }
			do { try files.removeExports(id) } catch { cleanupError = cleanupError ?? error }
			if let cleanupError {
				throw cleanupError
			}
		}
	}

	public func reset() async throws {
		var index = try files.loadIndex(); guard let id = index.activeID else { return }
		index.signedIn = false; index.reset = ResetJournal(profileID: id, phase: .stopping); try files.saveIndex(index)
		try await continueReset()
	}

	public func continueReset() async throws {
		var index = try files.loadIndex(); guard var journal = index.reset else { return }
		let generation = fence.reserve()
		do {
			await close()
			guard fence.currentGeneration() == generation else { throw SessionError.stale }
			if journal.phase == .stopping {
				try files.removeDatabase(journal.profileID); journal.phase = .databaseRemoved; index.reset = journal; try files
					.saveIndex(index)
			}
			if journal.phase == .databaseRemoved {
				try files.removeProfile(journal.profileID); journal.phase = .filesRemoved; index.reset = journal; try files
					.saveIndex(index)
			}
			try secrets.removeAll(journal.profileID); index.profiles.removeAll { $0.id == journal.profileID }; if index
				.activeID == journal.profileID
			{
				index.activeID = nil
			}; index.reset = nil; try files.saveIndex(index); apply(generation) { $0 = .signedOut }
		} catch { apply(generation) { $0 = .resetIncomplete }; throw error }
	}

	private func apply(_ generation: UInt64, _ change: (inout SessionState) -> Void) {
		guard fence.currentGeneration() == generation else { return }; change(&state)
	}
}
