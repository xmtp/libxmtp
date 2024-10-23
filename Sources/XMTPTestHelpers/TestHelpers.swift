//
//  TestHelpers.swift
//
//
//  Created by Pat Nakajima on 12/6/22.
//

#if canImport(XCTest)
import Combine
import CryptoKit
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

public struct FakeSCWWallet: SigningKey {
	public var walletAddress: String
	private var internalSignature: String
	
	public init() throws {
		// Simulate a wallet address (could be derived from a hash of some internal data)
		self.walletAddress = UUID().uuidString // Using UUID for uniqueness in this fake example
		self.internalSignature = Data(repeating: 0x01, count: 64).toHex // Fake internal signature
	}
	
	public var address: String {
		walletAddress
	}

	public var type: WalletType {
		WalletType.SCW
	}
	
	public var chainId: Int64? {
		1
	}
	
	public static func generate() throws -> FakeSCWWallet {
		return try FakeSCWWallet()
	}
	
	public func signSCW(message: String) async throws -> Data {
		// swiftlint:disable force_unwrapping
		let digest = SHA256.hash(data: message.data(using: .utf8)!)
		// swiftlint:enable force_unwrapping
		return Data(digest)
	}
}

@available(iOS 15, *)
public struct Fixtures {
	public var alice: PrivateKey!
	public var aliceClient: Client!

	public var bob: PrivateKey!
	public var bobClient: Client!
	public let clientOptions: ClientOptions? = ClientOptions(
		api: ClientOptions.Api(env: XMTPEnvironment.local, isSecure: false)
	)

	init() async throws {
		alice = try PrivateKey.generate()
		bob = try PrivateKey.generate()

		aliceClient = try await Client.create(account: alice, options: clientOptions)
		bobClient = try await Client.create(account: bob, options: clientOptions)
	}

	public func publishLegacyContact(client: Client) async throws {
		var contactBundle = ContactBundle()
		contactBundle.v1.keyBundle = try client.v1keys.toPublicKeyBundle()

		var envelope = Envelope()
		envelope.contentTopic = Topic.contact(client.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
		envelope.message = try contactBundle.serializedData()

		try await client.publish(envelopes: [envelope])
	}
}

public extension XCTestCase {
	@available(iOS 15, *)
	func fixtures() async -> Fixtures {
		// swiftlint:disable force_try
		return try! await Fixtures()
		// swiftlint:enable force_try
	}
}
#endif
