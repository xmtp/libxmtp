@testable import ExampleCore
import Foundation
import Testing

struct BackendProfileIdentityTests {
	@Test(arguments: [
		["HTTPS://EXAMPLE.COM:443", "https://example.com"],
		["HTTP://LOCALHOST:80/", "http://localhost"],
		["https://EXAMPLE.COM:443/API/v1/", "https://example.com/API/v1"],
	]) @MainActor
	func equivalentLegacyURLReusesIdentity(_ forms: [String]) async throws {
		let profile = BackendProfile(url: forms[0], inboxID: "saved-inbox", identity: "saved-identity")
		let files = IdentityFiles(profile: profile)
		let secrets = IdentitySecrets(profile: profile.id)
		let factory = IdentityFactory(saved: profile)
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		let originalSecrets = secrets.values
		try await session.connect(url: forms[1], token: "")
		let active = try #require(session.activeInfo)
		#expect(active.profile.id == profile.id)
		#expect(active.profile.url == profile.url)
		#expect(active.profile.inboxID == profile.inboxID)
		#expect(active.profile.identity == profile.identity)
		#expect(files.index.profiles.count == 1)
		#expect(files.index.activeID == profile.id)
		#expect(factory.openedProfile == profile.id)
		#expect(factory.openedWallet == originalSecrets[profile.id]?["wallet"])
		#expect(factory.openedDatabaseKey == originalSecrets[profile.id]?["database-key"])
		#expect(factory.openedRoot?.lastPathComponent == profile.id.uuidString)
		#expect(secrets.values == originalSecrets)
		#expect(secrets.randomCalls == 0)
		#expect(files.removals == 0)
	}

	@Test(arguments: ["https://example.com:444/API", "https://example.com/api"]) @MainActor
	func differentEndpointCreatesSeparateIdentity(_ input: String) async throws {
		let profile = BackendProfile(url: "https://EXAMPLE.COM:443/API", inboxID: "saved-inbox", identity: "saved-identity")
		let files = IdentityFiles(profile: profile)
		let secrets = IdentitySecrets(profile: profile.id)
		let factory = IdentityFactory(saved: profile)
		let session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
		let originalSecrets = secrets.values[profile.id]
		try await session.connect(url: input, token: "")
		let active = try #require(session.activeInfo)
		#expect(active.profile.id != profile.id)
		#expect(active.profile.url == input)
		#expect(active.profile.inboxID != profile.inboxID)
		#expect(files.index.profiles.count == 2)
		#expect(files.index.profiles.contains(profile))
		#expect(secrets.values[profile.id] == originalSecrets)
		#expect(files.removals == 0)
	}
}

private extension SessionController {
	var activeInfo: SessionInfo? {
		if case let .active(info) = state {
			return info
		}
		return nil
	}
}

@MainActor private final class IdentitySecrets: SecretStore {
	var values: [UUID: [String: Data]]
	var randomCalls = 0
	init(profile: UUID) {
		values = [profile: [
			"wallet": Data(repeating: 7, count: 32),
			"database-key": Data(repeating: 8, count: 32),
			"preferences-key": Data(repeating: 9, count: 32),
		]]
	}

	func read(_ profile: UUID, ref: String) throws -> Data? {
		values[profile]?[ref]
	}

	func write(_ profile: UUID, ref: String, data: Data) throws {
		values[profile, default: [:]][ref] = data
	}

	func remove(_ profile: UUID, ref: String) throws {
		values[profile]?[ref] = nil
	}

	func removeAll(_ profile: UUID) throws {
		values[profile] = nil
	}

	func randomBytes(count: Int) throws -> Data {
		randomCalls += 1; return Data(repeating: 1, count: count)
	}
}

@MainActor private final class IdentityFiles: ProfileFileStore {
	var index: SessionIndex
	let storedID: UUID
	var removals = 0
	init(profile: BackendProfile) {
		index = SessionIndex(); index.profiles = [profile]; storedID = profile.id
	}

	func loadIndex() throws -> SessionIndex {
		index
	}

	func saveIndex(_ index: SessionIndex) throws {
		self.index = index
	}

	func paths(_ profile: UUID) throws
		-> ProfilePaths
	{
		ProfilePaths(root: URL(fileURLWithPath: "/tmp/" + profile.uuidString))
	}

	func databaseExists(_ profile: UUID) throws -> Bool {
		profile == storedID
	}

	func removeDatabase(_: UUID) throws {
		removals += 1
	}

	func removeProfile(_: UUID) throws {
		removals += 1
	}

	func removeExports(_: UUID) throws {
		removals += 1
	}
}

@MainActor private final class IdentityClient: SessionClient {
	let inboxID: String
	let identity: String
	init(inboxID: String, identity: String) {
		self.inboxID = inboxID; self.identity = identity
	}

	func end() async throws {}
	func disableNotifications() async throws {}
}

@MainActor private final class IdentityFactory: SessionFactory {
	let saved: BackendProfile
	var openedProfile: UUID?
	var openedWallet: Data?
	var openedDatabaseKey: Data?
	var openedRoot: URL?
	init(saved: BackendProfile) {
		self.saved = saved
	}

	// Matches the explicit identity, storage and admission inputs of SessionFactory.
	// swiftlint:disable:next function_parameter_count
	func open(profile: BackendProfile, paths: ProfilePaths, wallet: Data, databaseKey: Data,
	          secrets _: any SecretStore, key _: SessionKey, fence _: SessionFence) async throws -> any SessionClient
	{
		openedProfile = profile.id; openedWallet = wallet; openedDatabaseKey = databaseKey; openedRoot = paths.root
		return IdentityClient(inboxID: profile.id == saved.id ? "saved-inbox" : "new-inbox",
		                      identity: profile.id == saved.id ? "saved-identity" : "new-identity")
	}
}
