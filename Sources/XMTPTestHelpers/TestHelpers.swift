#if canImport(XCTest)
	import Combine
	import XCTest
	@testable import XMTPiOS
	import LibXMTP

	public struct TestConfig {
		static let TEST_SERVER_ENABLED = _env("TEST_SERVER_ENABLED") == "true"
		// TODO: change Client constructor to accept these explicitly (so we can config CI):
		// static let TEST_SERVER_HOST = _env("TEST_SERVER_HOST") ?? "127.0.0.1"
		// static let TEST_SERVER_PORT = Int(_env("TEST_SERVER_PORT")) ?? 5556
		// static let TEST_SERVER_IS_SECURE = _env("TEST_SERVER_IS_SECURE") == "true"

		static private func _env(_ key: String) -> String? {
			ProcessInfo.processInfo.environment[key]
		}

		static public func skipIfNotRunningLocalNodeTests() throws {
			try XCTSkipIf(!TEST_SERVER_ENABLED, "requires local node")
		}

		static public func skip(because: String) throws {
			try XCTSkipIf(true, because)
		}
	}

	// Helper for tests gathering transcripts in a background task.
	public actor TestTranscript {
		public var messages: [String] = []
		public init() {}
		public func add(_ message: String) {
			messages.append(message)
		}
	}

	public struct FakeWallet: SigningKey {
		public static func generate() throws -> FakeWallet {
			let key = try PrivateKey.generate()
			return FakeWallet(key)
		}

		public var address: String {
			key.walletAddress
		}

		public func sign(_ data: Data) async throws -> XMTPiOS.Signature {
			let signature = try await key.sign(data)
			return signature
		}

		public func sign(message: String) async throws -> XMTPiOS.Signature {
			let signature = try await key.sign(message: message)
			return signature
		}

		public var key: PrivateKey

		public init(_ key: PrivateKey) {
			self.key = key
		}
	}

	@available(iOS 15, *)
	public struct Fixtures {
		public var alix: PrivateKey!
		public var alixClient: Client!
		public var bo: PrivateKey!
		public var boClient: Client!
		public var caro: PrivateKey!
		public var caroClient: Client!
		public var davon: PrivateKey!
		public var davonClient: Client!

		init(clientOptions: ClientOptions.Api) async throws {
			alix = try PrivateKey.generate()
			bo = try PrivateKey.generate()
			caro = try PrivateKey.generate()
			davon = try PrivateKey.generate()

			let key = try Crypto.secureRandomBytes(count: 32)
			let clientOptions: ClientOptions = ClientOptions(
				api: clientOptions,
				dbEncryptionKey: key
			)

			alixClient = try await Client.create(
				account: alix, options: clientOptions)
			boClient = try await Client.create(
				account: bo, options: clientOptions)
			caroClient = try await Client.create(
				account: caro, options: clientOptions)
			davonClient = try await Client.create(
				account: davon, options: clientOptions)
		}
	}

	extension XCTestCase {
		@available(iOS 15, *)
		public func fixtures(
			clientOptions: ClientOptions.Api = ClientOptions.Api(
				env: XMTPEnvironment.local, isSecure: false)
		) async throws -> Fixtures {
			return try await Fixtures(clientOptions: clientOptions)
		}
	}
#endif
