//
//  ApiClient.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import LibXMTP

public typealias PublishRequest = Xmtp_MessageApi_V1_PublishRequest
public typealias PublishResponse = Xmtp_MessageApi_V1_PublishResponse
public typealias BatchQueryRequest = Xmtp_MessageApi_V1_BatchQueryRequest
public typealias BatchQueryResponse = Xmtp_MessageApi_V1_BatchQueryResponse
public typealias Cursor = Xmtp_MessageApi_V1_Cursor
public typealias QueryRequest = Xmtp_MessageApi_V1_QueryRequest
public typealias QueryResponse = Xmtp_MessageApi_V1_QueryResponse
public typealias SubscribeRequest = Xmtp_MessageApi_V1_SubscribeRequest

// This protocol is in place to enable extending error handling for errors
// thrown via call sites to LibXMTP.FfiConverterTypeGenericError. Adopting
// the GenericErrorDescribing protocol will catch all instances of the enum
// and generate the string descriptions.
protocol GenericErrorDescribing {
	func generateApiDescription(error: GenericError) -> String
}

extension GenericErrorDescribing {
	func generateApiDescription(error: GenericError) -> String {
		switch error {
		case let .Client(message),
			let .ClientBuilder(message),
			let .Storage(message),
			let .ApiError(message),
			let .GroupError(message),
			let .Signature(message),
			let .GroupMetadata(message),
			let .Generic(message),
			let .GroupMutablePermissions(message),
			let .SignatureRequestError(message),
			let .Erc1271SignatureError(message):
			return message
		}
	}
}

struct ApiClientError: LocalizedError, GenericErrorDescribing {
	var errorDescription: String?

	init(error: GenericError, description: String) {
		self.errorDescription = "\(description) \(generateApiDescription(error: error))"
	}
}

protocol ApiClient: Sendable {
	var environment: XMTPEnvironment { get }
	init(environment: XMTPEnvironment, secure: Bool, rustClient: LibXMTP.FfiV2ApiClient, appVersion: String?) throws
	func setAuthToken(_ token: String)
	func batchQuery(request: BatchQueryRequest) async throws -> BatchQueryResponse
	func query(topic: String, pagination: Pagination?, cursor: Xmtp_MessageApi_V1_Cursor?) async throws -> QueryResponse
	func query(topic: Topic, pagination: Pagination?) async throws -> QueryResponse
	func query(request: QueryRequest) async throws -> QueryResponse
	func envelopes(topic: String, pagination: Pagination?) async throws -> [Envelope]
	func publish(envelopes: [Envelope]) async throws
	func publish(request: PublishRequest) async throws
	func subscribe(request: FfiV2SubscribeRequest, callback: FfiV2SubscriptionCallback) async throws -> FfiV2Subscription
}

func makeQueryRequest(topic: String, pagination: Pagination? = nil, cursor: Cursor? = nil) -> QueryRequest {
	return QueryRequest.with {
		$0.contentTopics = [topic]
		if let pagination {
			$0.pagingInfo = pagination.pagingInfo
		}
		if let endAt = pagination?.before {
			$0.endTimeNs = UInt64(endAt.millisecondsSinceEpoch) * 1_000_000
			$0.pagingInfo.direction = pagination?.direction ?? .descending
		}
		if let startAt = pagination?.after {
			$0.startTimeNs = UInt64(startAt.millisecondsSinceEpoch) * 1_000_000
			$0.pagingInfo.direction = pagination?.direction ?? .descending
		}
		if let cursor {
			$0.pagingInfo.cursor = cursor
		}
	}
}

final class GRPCApiClient: ApiClient {
	let ClientVersionHeaderKey = "X-Client-Version"
	let AppVersionHeaderKey = "X-App-Version"

	let environment: XMTPEnvironment
	var authToken = ""

	var rustClient: LibXMTP.FfiV2ApiClient

	required init(environment: XMTPEnvironment, secure _: Bool = true, rustClient: LibXMTP.FfiV2ApiClient, appVersion: String? = nil) throws {
		self.environment = environment
		self.rustClient = rustClient
		if let appVersion = appVersion {
			rustClient.setAppVersion(version: appVersion)
		}
	}

	func setAuthToken(_ token: String) {
		authToken = token
	}

	func batchQuery(request: BatchQueryRequest) async throws -> BatchQueryResponse {
		do {
			return try await rustClient.batchQuery(req: request.toFFI).fromFFI
		} catch let error as GenericError {
			throw ApiClientError(error: error, description: "ApiClientError.batchQueryError:")
		}
	}

	func query(request: QueryRequest) async throws -> QueryResponse {
		do {
			return try await rustClient.query(request: request.toFFI).fromFFI
		} catch let error as GenericError {
			throw ApiClientError(error: error, description: "ApiClientError.queryError:")
		}
	}

	func query(topic: String, pagination: Pagination? = nil, cursor: Cursor? = nil) async throws -> QueryResponse {
		return try await query(request: makeQueryRequest(topic: topic, pagination: pagination, cursor: cursor))
	}

	func query(topic: Topic, pagination: Pagination? = nil) async throws -> QueryResponse {
		return try await query(request: makeQueryRequest(topic: topic.description, pagination: pagination))
	}

	func envelopes(topic: String, pagination: Pagination? = nil) async throws -> [Envelope] {
		var envelopes: [Envelope] = []
		var hasNextPage = true
		var cursor: Xmtp_MessageApi_V1_Cursor?

		while hasNextPage {
			let response = try await query(topic: topic, pagination: pagination, cursor: cursor)

			envelopes.append(contentsOf: response.envelopes)

			cursor = response.pagingInfo.cursor
			hasNextPage = !response.envelopes.isEmpty && response.pagingInfo.hasCursor

			if let limit = pagination?.limit, envelopes.count >= limit, limit <= 100 {
				envelopes = Array(envelopes.prefix(limit))
				break
			}
		}

		return envelopes
	}
	
	func subscribe(
		request: FfiV2SubscribeRequest,
		callback: FfiV2SubscriptionCallback
	) async throws -> FfiV2Subscription {
		return try await rustClient.subscribe(request: request, callback: callback)
	}

	func publish(request: PublishRequest) async throws {
		do {
			try await rustClient.publish(request: request.toFFI, authToken: authToken)
		} catch let error as GenericError {
			throw ApiClientError(error: error, description: "ApiClientError.publishError:")
		}
	}

	func publish(envelopes: [Envelope]) async throws {
		return try await publish(request: PublishRequest.with {
			$0.envelopes = envelopes
		})
	}
}
