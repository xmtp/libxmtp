//
//  TestHelpers.swift
//
//
//  Created by Pat Nakajima on 12/6/22.
//

import Combine
import XCTest
@testable import XMTP

enum FakeApiClientError: String, Error {
	case noResponses
}

class FakeStreamHolder: ObservableObject {
	@Published var envelope: Envelope?

	func send(envelope: Envelope) {
		self.envelope = envelope
	}
}

@available(iOS 15, *)
class FakeApiClient: ApiClient {
	var environment: Environment
	var authToken: String = ""
	private var responses: [String: [Envelope]] = [:]

	private var stream = FakeStreamHolder()
	var published: [Envelope] = []
	var cancellable: AnyCancellable?

	deinit {
		cancellable?.cancel()
	}

	func assertNoPublish(callback: () async throws -> Void) async throws {
		let oldCount = published.count
		try await callback()
		XCTAssertEqual(oldCount, published.count, "Published messages: \(try? published[oldCount - 1 ..< published.count].map { try $0.jsonString() })")
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

	required init(environment: XMTP.Environment, secure _: Bool) throws {
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

	func query(topics: [String]) async throws -> XMTP.QueryResponse {
		var result: [Envelope] = []

		for topic in topics {
			if let response = responses.removeValue(forKey: topic) {
				result.append(contentsOf: response)
			}

			if let envelope = findPublishedEnvelope(topic) {
				result.append(envelope)
			}
		}

		var queryResponse = QueryResponse()
		queryResponse.envelopes = result

		return queryResponse
	}

	func query(topics: [XMTP.Topic]) async throws -> XMTP.QueryResponse {
		return try await query(topics: topics.map(\.description))
	}

	func publish(envelopes: [XMTP.Envelope]) async throws -> XMTP.PublishResponse {
		published.append(contentsOf: envelopes)

		return PublishResponse()
	}
}
