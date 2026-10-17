@testable import ExampleCore
import Foundation
import Testing

@MainActor struct StartupIndexRecoveryTests {
	@Test func repairedUnsignedIndexCanReturnToJoinWithoutWritingData() async {
		let files = StartupRecoveryFiles(), factory = StartupRecoveryFactory()
		let session = SessionController(
			fence: SessionFence(),
			secrets: StartupRecoverySecrets(),
			files: files,
			factory: factory,
		)
		await session.restore()
		#expect(session.state == .restoreFailed)
		files.unreadable = false
		await session.retryRestore()
		#expect(session.state == .signedOut)
		#expect(files.index.appearance == "dark")
		#expect(files.writes == 0)
		#expect(factory.opens == 0)
	}

	@Test(arguments: [false, true]) func retryPreservesSignedInFailureAndResetJournal(reset: Bool) async {
		let files = StartupRecoveryFiles(), factory = StartupRecoveryFactory()
		let session = SessionController(
			fence: SessionFence(),
			secrets: StartupRecoverySecrets(),
			files: files,
			factory: factory,
		)
		await session.restore()
		files.unreadable = false
		files.index.signedIn = !reset
		if reset {
			files.index.reset = ResetJournal(profileID: UUID(), phase: .stopping)
		}
		await session.retryRestore()
		#expect(session.state == (reset ? .resetIncomplete : .restoreFailed))
		#expect(files.writes == 0)
		#expect(factory.opens == 0)
	}
}

private enum StartupRecoveryError: Error { case unreadable, unexpectedOpen }
@MainActor private final class StartupRecoveryFiles: ProfileFileStore {
	var unreadable = true
	var index: SessionIndex = {
		var value = SessionIndex(); value.appearance = "dark"; return value
	}()

	var writes = 0
	func loadIndex() throws -> SessionIndex {
		if unreadable {
			throw StartupRecoveryError.unreadable
		}
		return index
	}

	func saveIndex(_ index: SessionIndex) throws {
		writes += 1; self.index = index
	}

	func paths(_ profile: UUID) throws
		-> ProfilePaths
	{
		ProfilePaths(root: URL(fileURLWithPath: "/owned-fixture/" + profile.uuidString))
	}

	func databaseExists(_: UUID) throws -> Bool {
		false
	}

	func removeDatabase(_: UUID) throws {
		throw StartupRecoveryError.unexpectedOpen
	}

	func removeExports(_: UUID) throws {
		throw StartupRecoveryError.unexpectedOpen
	}

	func removeProfile(_: UUID) throws {
		throw StartupRecoveryError.unexpectedOpen
	}
}

@MainActor private final class StartupRecoverySecrets: SecretStore {
	func read(_: UUID, ref _: String) throws -> Data? {
		nil
	}

	func write(_: UUID, ref _: String, data _: Data) throws {
		throw StartupRecoveryError.unexpectedOpen
	}

	func remove(_: UUID, ref _: String) throws {
		throw StartupRecoveryError.unexpectedOpen
	}

	func removeAll(_: UUID) throws {
		throw StartupRecoveryError.unexpectedOpen
	}

	func randomBytes(count: Int) throws -> Data {
		Data(repeating: 0, count: count)
	}
}

@MainActor private final class StartupRecoveryFactory: SessionFactory {
	var opens = 0
	// SessionFactory requires these seven ownership inputs, as in the production conformance.
	// swiftlint:disable:next function_parameter_count
	func open(profile _: BackendProfile, paths _: ProfilePaths, wallet _: Data, databaseKey _: Data,
	          secrets _: any SecretStore, key _: SessionKey, fence _: SessionFence) async throws -> any SessionClient
	{
		opens += 1; throw StartupRecoveryError.unexpectedOpen
	}
}
