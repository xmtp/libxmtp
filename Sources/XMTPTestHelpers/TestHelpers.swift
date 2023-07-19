//
//  TestHelpers.swift
//
//
//  Created by Pat Nakajima on 12/6/22.
//

#if canImport(XCTest)
import Combine
import XCTest
@testable import XMTP
import XMTPRust

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

	public func sign(_ data: Data) async throws -> XMTP.Signature {
		let signature = try await key.sign(data)
		return signature
	}

	public func sign(message: String) async throws -> XMTP.Signature {
		let signature = try await key.sign(message: message)
		return signature
	}

	public var key: PrivateKey

	public init(_ key: PrivateKey) {
		self.key = key
	}
}

enum FakeApiClientError: String, Error {
	case noResponses, queryAssertionFailure
}

class FakeStreamHolder: ObservableObject {
	@Published var envelope: XMTP.Envelope?

	func send(envelope: XMTP.Envelope) {
		self.envelope = envelope
	}
}

@available(iOS 15, *)
public class FakeApiClient: ApiClient {
	public func envelopes(topic: String, pagination: XMTP.Pagination?) async throws -> [XMTP.Envelope] {
		try await query(topic: topic, pagination: pagination).envelopes
	}

	public var environment: XMTPEnvironment
	public var authToken: String = ""
	private var responses: [String: [XMTP.Envelope]] = [:]
	private var stream = FakeStreamHolder()
	public var published: [XMTP.Envelope] = []
	var cancellable: AnyCancellable?
	var forbiddingQueries = false

	deinit {
		cancellable?.cancel()
	}

	public func assertNoPublish(callback: () async throws -> Void) async throws {
		let oldCount = published.count
		try await callback()
		// swiftlint:disable no_optional_try
		XCTAssertEqual(oldCount, published.count, "Published messages: \(String(describing: try? published[oldCount - 1 ..< published.count].map { try $0.jsonString() }))")
		// swiftlint:enable no_optional_try
	}

	public func assertNoQuery(callback: () async throws -> Void) async throws {
		forbiddingQueries = true
		try await callback()
		forbiddingQueries = false
	}

	public func register(message: [XMTP.Envelope], for topic: Topic) {
		var responsesForTopic = responses[topic.description] ?? []
		responsesForTopic.append(contentsOf: message)
		responses[topic.description] = responsesForTopic
	}

	public init() {
		environment = .local
	}

	public func send(envelope: XMTP.Envelope) {
		stream.send(envelope: envelope)
	}

	public func findPublishedEnvelope(_ topic: Topic) -> XMTP.Envelope? {
		return findPublishedEnvelope(topic.description)
	}

	public func findPublishedEnvelope(_ topic: String) -> XMTP.Envelope? {
		return published.reversed().first { $0.contentTopic == topic.description }
	}

	// MARK: ApiClient conformance

	public required init(environment: XMTP.XMTPEnvironment, secure _: Bool, rustClient _: XMTPRust.RustClient) throws {
		self.environment = environment
	}

	public func subscribe(topics: [String]) -> AsyncThrowingStream<XMTP.Envelope, Error> {
		AsyncThrowingStream { continuation in
			self.cancellable = stream.$envelope.sink(receiveValue: { env in
				if let env, topics.contains(env.contentTopic) {
					continuation.yield(env)
				}
			})
		}
	}

	public func setAuthToken(_ token: String) {
		authToken = token
	}

	public func query(topic: String, pagination: Pagination? = nil, cursor _: Xmtp_MessageApi_V1_Cursor? = nil) async throws -> XMTP.QueryResponse {
		if forbiddingQueries {
			XCTFail("Attempted to query \(topic)")
			throw FakeApiClientError.queryAssertionFailure
		}

		var result: [XMTP.Envelope] = []

		if let response = responses.removeValue(forKey: topic) {
			result.append(contentsOf: response)
		}

		result.append(contentsOf: published.filter { $0.contentTopic == topic }.reversed())

		if let startAt = pagination?.after {
			result = result
				.filter { $0.timestampNs > UInt64(startAt.millisecondsSinceEpoch * 1_000_000) }
		}

		if let endAt = pagination?.before {
			result = result
				.filter { $0.timestampNs < UInt64(endAt.millisecondsSinceEpoch * 1_000_000) }
		}

		if let limit = pagination?.limit {
			if limit == 1 {
				if let first = result.first {
					result = [first]
				} else {
					result = []
				}
			} else {
				let maxBound = min(result.count, limit) - 1

				if maxBound <= 0 {
					result = []
				} else {
					result = Array(result[0 ... maxBound])
				}
			}
		}

		var queryResponse = QueryResponse()
		queryResponse.envelopes = result

		return queryResponse
	}

	public func query(topic: XMTP.Topic, pagination: Pagination? = nil) async throws -> XMTP.QueryResponse {
		return try await query(topic: topic.description, pagination: pagination, cursor: nil)
	}

	public func publish(envelopes: [XMTP.Envelope]) async throws -> XMTP.PublishResponse {
		for envelope in envelopes {
			send(envelope: envelope)
		}

		published.append(contentsOf: envelopes)

		return PublishResponse()
	}

    public func batchQuery(request: XMTP.BatchQueryRequest) async throws -> XMTP.BatchQueryResponse {
        let responses = try await withThrowingTaskGroup(of: QueryResponse.self) { group in
            for r in request.requests {
                group.addTask {
                    try await self.query(topic: r.contentTopics[0], pagination: Pagination(after: Date(timeIntervalSince1970: Double(r.startTimeNs / 1_000_000) / 1000)))
                }
            }

          var results: [QueryResponse] = []
          for try await response in group {
            results.append(response)
          }

          return results
        }

        var queryResponse = XMTP.BatchQueryResponse()
        queryResponse.responses = responses
        return queryResponse
     
    }

    public func query(request: XMTP.QueryRequest) async throws -> XMTP.QueryResponse {
        abort() // Not supported on Fake
    }

    public func publish(request: XMTP.PublishRequest) async throws -> XMTP.PublishResponse {
        abort() // Not supported on Fake
    }

}

@available(iOS 15, *)
public struct Fixtures {
	public var fakeApiClient: FakeApiClient!

	public var alice: PrivateKey!
	public var aliceClient: Client!

	public var bob: PrivateKey!
	public var bobClient: Client!

	init() async throws {
		alice = try PrivateKey.generate()
		bob = try PrivateKey.generate()

		fakeApiClient = FakeApiClient()

		aliceClient = try await Client.create(account: alice, apiClient: fakeApiClient)
		bobClient = try await Client.create(account: bob, apiClient: fakeApiClient)
	}

	public func publishLegacyContact(client: Client) async throws {
		var contactBundle = ContactBundle()
		contactBundle.v1.keyBundle = client.privateKeyBundleV1.toPublicKeyBundle()

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
