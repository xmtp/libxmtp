//
//  ApiClient.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import GRPC
import XMTPProto

typealias PublishResponse = Xmtp_MessageApi_V1_PublishResponse

public struct ApiClient {
	let ClientVersionHeaderKey = "X-Client-Version"
	let AppVersionHeaderKey = "X-App-Version"

	var environment: Environment
	var authToken = ""

	private var client: Xmtp_MessageApi_V1_MessageApiAsyncClient!

	init(environment: Environment, secure: Bool = true) throws {
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

	mutating func setAuthToken(_ token: String) {
		authToken = token
	}

	func query(topics: [String]) async throws -> Xmtp_MessageApi_V1_QueryResponse {
		var request = Xmtp_MessageApi_V1_QueryRequest()
		request.contentTopics = topics

		var options = CallOptions()
		options.customMetadata.add(name: "authorization", value: "Bearer \(authToken)")
		options.timeLimit = .timeout(.seconds(5))

		return try await client.query(request, callOptions: options)
	}

	func query(topics: [Topic]) async throws -> Xmtp_MessageApi_V1_QueryResponse {
		return try await query(topics: topics.map(\.description))
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
