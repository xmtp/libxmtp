//
//  TestHelpers.swift
//
//
//  Created by Pat Nakajima on 12/6/22.
//

import Combine
import XCTest
@testable import XMTP

struct FakeWallet: SigningKey {
	static func generate() throws -> FakeWallet {
		let key = try PrivateKey.generate()
		return FakeWallet(key)
	}

	var address: String {
		key.walletAddress
	}

	func sign(_ data: Data) async throws -> XMTP.Signature {
		let signature = try await key.sign(data)
		return signature
	}

	func sign(message: String) async throws -> XMTP.Signature {
		let signature = try await key.sign(message: message)
		return signature
	}

	var key: PrivateKey

	init(_ key: PrivateKey) {
		self.key = key
	}
}

enum FakeApiClientError: String, Error {
	case noResponses, queryAssertionFailure
}

class FakeStreamHolder: ObservableObject {
	@Published var envelope: Envelope?

	func send(envelope: Envelope) {
		self.envelope = envelope
	}
}

@available(iOS 15, *)
class FakeApiClient: ApiClient {
	var environment: XMTPEnvironment
	var authToken: String = ""
	private var responses: [String: [Envelope]] = [:]
	private var stream = FakeStreamHolder()
	var published: [Envelope] = []
	var cancellable: AnyCancellable?
	var forbiddingQueries = false

	deinit {
		cancellable?.cancel()
	}

	func assertNoPublish(callback: () async throws -> Void) async throws {
		let oldCount = published.count
		try await callback()
		XCTAssertEqual(oldCount, published.count, "Published messages: \(try? published[oldCount - 1 ..< published.count].map { try $0.jsonString() })")
	}

	func assertNoQuery(callback: () async throws -> Void) async throws {
		forbiddingQueries = true
		try await callback()
		forbiddingQueries = false
	}

	func register(message: [Envelope], for topic: Topic) {
		var responsesForTopic = responses[topic.description] ?? []
		responsesForTopic.append(contentsOf: message)
		responses[topic.description] = responsesForTopic
	}

	init() {
		environment = .local
	}

	func send(envelope: Envelope) {
		stream.send(envelope: envelope)
	}

	func findPublishedEnvelope(_ topic: Topic) -> Envelope? {
		return findPublishedEnvelope(topic.description)
	}

	func findPublishedEnvelope(_ topic: String) -> Envelope? {
		for envelope in published.reversed() {
			if envelope.contentTopic == topic.description {
				return envelope
			}
		}

		return nil
	}

	// MARK: ApiClient conformance

	required init(environment: XMTP.XMTPEnvironment, secure _: Bool) throws {
		self.environment = environment
	}

	func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error> {
		AsyncThrowingStream { continuation in
			self.cancellable = stream.$envelope.sink(receiveValue: { env in
				if let env, topics.contains(env.contentTopic) {
					continuation.yield(env)
				}
			})
		}
	}

	func setAuthToken(_ token: String) {
		authToken = token
	}

	func query(topics: [String], pagination: Pagination? = nil) async throws -> XMTP.QueryResponse {
		if forbiddingQueries {
			XCTFail("Attempted to query \(topics)")
			throw FakeApiClientError.queryAssertionFailure
		}

		var result: [Envelope] = []

		for topic in topics {
			if let response = responses.removeValue(forKey: topic) {
				result.append(contentsOf: response)
			}

			result.append(contentsOf: published.filter { $0.contentTopic == topic }.reversed())
		}

		if let startAt = pagination?.startTime {
			result = result
				.filter { $0.timestampNs < UInt64(startAt.millisecondsSinceEpoch * 1_000_000) }
				.sorted(by: { $0.timestampNs > $1.timestampNs })
		}

		if let endAt = pagination?.endTime {
			result = result
				.filter { $0.timestampNs > UInt64(endAt.millisecondsSinceEpoch * 1_000_000) }
				.sorted(by: { $0.timestampNs < $1.timestampNs })
		}

		if let limit = pagination?.limit {
			if limit == 1 {
				if let first = result.first {
					result = [first]
				} else {
					result = []
				}
			} else {
				result = Array(result[0 ... limit - 1])
			}
		}

		var queryResponse = QueryResponse()
		queryResponse.envelopes = result

		return queryResponse
	}

	func query(topics: [XMTP.Topic], pagination: Pagination? = nil) async throws -> XMTP.QueryResponse {
		return try await query(topics: topics.map(\.description), pagination: pagination)
	}

	func publish(envelopes: [XMTP.Envelope]) async throws -> XMTP.PublishResponse {
		for envelope in envelopes {
			send(envelope: envelope)
		}

		published.append(contentsOf: envelopes)

		return PublishResponse()
	}
}

@available(iOS 15, *)
struct Fixtures {
	var fakeApiClient: FakeApiClient!

	var alice: PrivateKey!
	var aliceClient: Client!

	var bob: PrivateKey!
	var bobClient: Client!

	init() async throws {
		alice = try PrivateKey.generate()
		bob = try PrivateKey.generate()

		fakeApiClient = FakeApiClient()

		aliceClient = try await Client.create(account: alice, apiClient: fakeApiClient)
		bobClient = try await Client.create(account: bob, apiClient: fakeApiClient)
	}
}

extension XCTestCase {
	@available(iOS 15, *)
	func fixtures() async -> Fixtures {
		return try! await Fixtures()
	}
}
