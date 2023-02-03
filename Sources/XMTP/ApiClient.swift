//
//  ApiClient.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import GRPC
import XMTPProto

typealias PublishResponse = Xmtp_MessageApi_V1_PublishResponse
typealias QueryResponse = Xmtp_MessageApi_V1_QueryResponse
typealias SubscribeRequest = Xmtp_MessageApi_V1_SubscribeRequest

protocol ApiClient {
	var environment: XMTPEnvironment { get }
	init(environment: XMTPEnvironment, secure: Bool) throws
	func setAuthToken(_ token: String)
	func query(topics: [String], pagination: Pagination?, cursor: Xmtp_MessageApi_V1_Cursor?) async throws -> QueryResponse
	func query(topics: [Topic], pagination: Pagination?) async throws -> QueryResponse
	func envelopes(topics: [String], pagination: Pagination?) async throws -> [Envelope]
	func publish(envelopes: [Envelope]) async throws -> PublishResponse
	func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error>
}

class GRPCApiClient: ApiClient {
	let ClientVersionHeaderKey = "X-Client-Version"
	let AppVersionHeaderKey = "X-App-Version"

	var environment: XMTPEnvironment
	var authToken = ""

	private var client: Xmtp_MessageApi_V1_MessageApiAsyncClient!

	required init(environment: XMTPEnvironment, secure: Bool = true) throws {
		self.environment = environment
		let group = PlatformSupport.makeEventLoopGroup(loopCount: 1)

		let config = GRPCTLSConfiguration.makeClientConfigurationBackedByNIOSSL()
		let channel = try GRPCChannelPool.with(
			target: .host(environment.rawValue, port: 5556),
			transportSecurity: secure ? .tls(config) : .plaintext,
			eventLoopGroup: group
		)

		client = Xmtp_MessageApi_V1_MessageApiAsyncClient(channel: channel)
	}

	func setAuthToken(_ token: String) {
		authToken = token
	}

	func query(topics: [String], pagination: Pagination? = nil, cursor: Xmtp_MessageApi_V1_Cursor? = nil) async throws -> QueryResponse {
		var request = Xmtp_MessageApi_V1_QueryRequest()
		request.contentTopics = topics

		if let pagination {
			request.pagingInfo = pagination.pagingInfo
		}

		if let startAt = pagination?.startTime {
			request.endTimeNs = UInt64(startAt.millisecondsSinceEpoch) * 1_000_000
			request.pagingInfo.direction = .descending
		}

		if let endAt = pagination?.endTime {
			request.startTimeNs = UInt64(endAt.millisecondsSinceEpoch) * 1_000_000
			request.pagingInfo.direction = .descending
		}

		if let cursor {
			request.pagingInfo.cursor = cursor
		}

		var options = CallOptions()
		options.customMetadata.add(name: "authorization", value: "Bearer \(authToken)")
		options.timeLimit = .timeout(.seconds(5))

		return try await client.query(request, callOptions: options)
	}

	func envelopes(topics: [String], pagination: Pagination? = nil) async throws -> [Envelope] {
		var envelopes: [Envelope] = []
		var hasNextPage = true
		var cursor: Xmtp_MessageApi_V1_Cursor?

		while hasNextPage {
			let response = try await query(topics: topics, pagination: pagination, cursor: cursor)

			envelopes.append(contentsOf: response.envelopes)

			cursor = response.pagingInfo.cursor
			hasNextPage = !response.envelopes.isEmpty && response.pagingInfo.hasCursor
		}

		return envelopes
	}

	func query(topics: [Topic], pagination: Pagination? = nil) async throws -> Xmtp_MessageApi_V1_QueryResponse {
		return try await query(topics: topics.map(\.description), pagination: pagination)
	}

	func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error> {
		return AsyncThrowingStream { continuation in
			Task {
				var request = SubscribeRequest()
				request.contentTopics = topics

				for try await envelope in self.client.subscribe(request) {
					continuation.yield(envelope)
				}
			}
		}
	}

	@discardableResult func publish(envelopes: [Envelope]) async throws -> PublishResponse {
		var request = Xmtp_MessageApi_V1_PublishRequest()
		request.envelopes = envelopes

		var options = CallOptions()
		options.customMetadata.add(name: "authorization", value: "Bearer \(authToken)")
		options.customMetadata.add(name: ClientVersionHeaderKey, value: Constants.version)
		options.customMetadata.add(name: AppVersionHeaderKey, value: Constants.version)
		options.timeLimit = .timeout(.seconds(5))

		return try await client.publish(request, callOptions: options)
	}
}
