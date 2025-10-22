#if canImport(XCTest)
	import Combine
	import Foundation
	import XCTest
	import XMTPiOS

	func _env(_ key: String) -> String? {
		ProcessInfo.processInfo.environment[key]
	}

	public func getLocalAddressFromEnvironment() -> String? {
		_env("XMTP_NODE_ADDRESS")
	}

	public func getHistorySyncUrlFromEnvironment() -> String? {
		_env("XMTP_HISTORY_SERVER_ADDRESS")
	}

	public enum TestConfig {
		static let TEST_SERVER_ENABLED = _env("TEST_SERVER_ENABLED") == "true"
		// TODO: change Client constructor to accept these explicitly (so we can config CI):
		// static let TEST_SERVER_HOST = _env("TEST_SERVER_HOST") ?? "127.0.0.1"
		// static let TEST_SERVER_PORT = Int(_env("TEST_SERVER_PORT")) ?? 5556
		// static let TEST_SERVER_IS_SECURE = _env("TEST_SERVER_IS_SECURE") == "true"

		public static func skipIfNotRunningLocalNodeTests() throws {
			try XCTSkipIf(!TEST_SERVER_ENABLED, "requires local node")
		}

		public static func skip(because: String) throws {
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

			let key = Data((0 ..< 32).map { _ in UInt8.random(in: 0 ... 255) })
			let clientOptions = ClientOptions(
				api: clientOptions,
				dbEncryptionKey: key
			)

			alixClient = try await Client.create(
				account: alix, options: clientOptions
			)
			boClient = try await Client.create(
				account: bo, options: clientOptions
			)
			caroClient = try await Client.create(
				account: caro, options: clientOptions
			)
			davonClient = try await Client.create(
				account: davon, options: clientOptions
			)
		}

		public func cleanUpDatabases() throws {
			for client in [alixClient, boClient, caroClient, davonClient] {
				try client?.deleteLocalDatabase()
			}
		}
	}

	public extension XCTestCase {
		func setupLocalEnv() {
			if let localAddress = getLocalAddressFromEnvironment(), !localAddress.isEmpty {
				XMTPEnvironment.customLocalAddress = localAddress
			}
			if let localHistoryUrl = getHistorySyncUrlFromEnvironment(), !localHistoryUrl.isEmpty {
				XMTPEnvironment.customHistorySyncUrl = localHistoryUrl
			}
		}

		@available(iOS 15, *)
		func fixtures(
			clientOptions: ClientOptions.Api = ClientOptions.Api(
				env: XMTPEnvironment.local, isSecure: XMTPEnvironment.local.isSecure
			)
		) async throws -> Fixtures {
			try await Fixtures(clientOptions: clientOptions)
		}
	}
#endif
