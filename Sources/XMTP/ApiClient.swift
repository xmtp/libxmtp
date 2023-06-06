//
//  ApiClient.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import XMTPRust
import XMTPRustSwift

typealias PublishRequest = Xmtp_MessageApi_V1_PublishRequest
typealias PublishResponse = Xmtp_MessageApi_V1_PublishResponse
typealias BatchQueryRequest = Xmtp_MessageApi_V1_BatchQueryRequest
typealias BatchQueryResponse = Xmtp_MessageApi_V1_BatchQueryResponse
typealias Cursor = Xmtp_MessageApi_V1_Cursor
typealias QueryRequest = Xmtp_MessageApi_V1_QueryRequest
typealias QueryResponse = Xmtp_MessageApi_V1_QueryResponse
typealias SubscribeRequest = Xmtp_MessageApi_V1_SubscribeRequest

protocol ApiClient {
	var environment: XMTPEnvironment { get }
	init(environment: XMTPEnvironment, secure: Bool, rustClient: XMTPRust.RustClient) throws
	func setAuthToken(_ token: String)
    func batchQuery(request: BatchQueryRequest) async throws -> BatchQueryResponse
	func query(topic: String, pagination: Pagination?, cursor: Xmtp_MessageApi_V1_Cursor?) async throws -> QueryResponse
	func query(topic: Topic, pagination: Pagination?) async throws -> QueryResponse
    func query(request: QueryRequest) async throws -> QueryResponse
	func envelopes(topic: String, pagination: Pagination?) async throws -> [Envelope]
	func publish(envelopes: [Envelope]) async throws -> PublishResponse
    func publish(request: PublishRequest) async throws -> PublishResponse
	func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error>
}

func makeQueryRequest(topic: String, pagination: Pagination? = nil, cursor: Cursor? = nil) -> QueryRequest {
    return QueryRequest.with {
        $0.contentTopics = [topic]
        if let pagination {
            $0.pagingInfo = pagination.pagingInfo
        }
        if let startAt = pagination?.startTime {
            $0.endTimeNs = UInt64(startAt.millisecondsSinceEpoch) * 1_000_000
            $0.pagingInfo.direction = .descending
        }
        if let endAt = pagination?.endTime {
            $0.startTimeNs = UInt64(endAt.millisecondsSinceEpoch) * 1_000_000
            $0.pagingInfo.direction = .descending
        }
        if let cursor {
            $0.pagingInfo.cursor = cursor
        }
    }
}

class GRPCApiClient: ApiClient {
	let ClientVersionHeaderKey = "X-Client-Version"
	let AppVersionHeaderKey = "X-App-Version"

	var environment: XMTPEnvironment
	var authToken = ""

	var rustClient: XMTPRust.RustClient

	required init(environment: XMTPEnvironment, secure _: Bool = true, rustClient: XMTPRust.RustClient) throws {
		self.environment = environment
		self.rustClient = rustClient
	}

	static func envToUrl(env: XMTPEnvironment) -> String {
		switch env {
		case XMTPEnvironment.local: return "http://localhost:5556"
		case XMTPEnvironment.dev: return "https://dev.xmtp.network:5556"
		case XMTPEnvironment.production: return "https://production.xmtp.network:5556"
		}
	}

	func setAuthToken(_ token: String) {
		authToken = token
	}

    func batchQuery(request: BatchQueryRequest) async throws -> BatchQueryResponse {
        let req = RustVec<UInt8>(try request.serializedData())
        let res: RustVec<UInt8> = try await rustClient.batch_query(req)
        return try BatchQueryResponse(serializedData: Data(res))
    }

    func query(request: QueryRequest) async throws -> QueryResponse {
        let req = RustVec<UInt8>(try request.serializedData())
        let res: RustVec<UInt8> = try await rustClient.query(req)
        return try QueryResponse(serializedData: Data(res))
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
		}

		return envelopes
	}

	func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error> {
		return AsyncThrowingStream { continuation in
			Task {
                let request = SubscribeRequest.with { $0.contentTopics = topics }
                let req = RustVec<UInt8>(try request.serializedData())
                let subscription = try await self.rustClient.subscribe(req)
				// Run a continuous for loop polling and sleeping for a bit each loop.
				while true {
					let buf = try subscription.get_envelopes_as_query_response()
                    // Note: it uses QueryResponse as a convenient envelopes wrapper.
                    let res = try QueryResponse(serializedData: Data(buf))
                    for envelope in res.envelopes {
						continuation.yield(envelope)
					}
					try await Task.sleep(nanoseconds: 50_000_000) // 50ms
				}
			}
		}
	}

    func publish(request: PublishRequest) async throws -> PublishResponse {
        let req = RustVec<UInt8>(try request.serializedData())
        let res: RustVec<UInt8> = try await rustClient.publish(authToken.intoRustString(), req)
        return try PublishResponse(serializedData: Data(res))
    }

	@discardableResult func publish(envelopes: [Envelope]) async throws -> PublishResponse {
        return try await publish(request: PublishRequest.with {
            $0.envelopes = envelopes
        })
	}
}
