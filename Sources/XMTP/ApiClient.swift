//
//  ApiClient.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import XMTPRust
import XMTPRustSwift

public typealias PublishRequest = Xmtp_MessageApi_V1_PublishRequest
public typealias PublishResponse = Xmtp_MessageApi_V1_PublishResponse
public typealias BatchQueryRequest = Xmtp_MessageApi_V1_BatchQueryRequest
public typealias BatchQueryResponse = Xmtp_MessageApi_V1_BatchQueryResponse
public typealias Cursor = Xmtp_MessageApi_V1_Cursor
public typealias QueryRequest = Xmtp_MessageApi_V1_QueryRequest
public typealias QueryResponse = Xmtp_MessageApi_V1_QueryResponse
public typealias SubscribeRequest = Xmtp_MessageApi_V1_SubscribeRequest

public enum ApiClientError: Error {
    case batchQueryError(String)
    case queryError(String)
    case publishError(String)
    case subscribeError(String)
}

protocol ApiClient {
	var environment: XMTPEnvironment { get }
    init(environment: XMTPEnvironment, secure: Bool, rustClient: XMTPRust.RustClient, appVersion: String?) throws
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

class GRPCApiClient: ApiClient {
	let ClientVersionHeaderKey = "X-Client-Version"
	let AppVersionHeaderKey = "X-App-Version"

	var environment: XMTPEnvironment
	var authToken = ""

	var rustClient: XMTPRust.RustClient

    required init(environment: XMTPEnvironment, secure _: Bool = true, rustClient: XMTPRust.RustClient, appVersion: String? = nil) throws {
		self.environment = environment
		self.rustClient = rustClient
        if let appVersion = appVersion {
            rustClient.set_app_version(appVersion.intoRustString())
        }
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
        do {
            let req = RustVec<UInt8>(try request.serializedData())
            let res: RustVec<UInt8> = try await rustClient.batch_query(req)
            return try BatchQueryResponse(serializedData: Data(res))
        } catch let error as RustString {
            throw ApiClientError.batchQueryError(error.toString())
        }
    }

    func query(request: QueryRequest) async throws -> QueryResponse {
        do {
            let req = RustVec<UInt8>(try request.serializedData())
            let res: RustVec<UInt8> = try await rustClient.query(req)
            return try QueryResponse(serializedData: Data(res))
        } catch let error as RustString {
            throw ApiClientError.queryError(error.toString())
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
            
            if let limit = pagination?.limit, envelopes.count >= limit {
                envelopes = Array(envelopes.prefix(limit))
                break
            }
		}

		return envelopes
	}

	func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error> {
		return AsyncThrowingStream { continuation in
			Task {
                let request = SubscribeRequest.with { $0.contentTopics = topics }
                let req = RustVec<UInt8>(try request.serializedData())
                do {
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
                } catch let error as RustString {
                    throw ApiClientError.subscribeError(error.toString())
                }
			}
		}
	}

    func publish(request: PublishRequest) async throws -> PublishResponse {
        do {
            let req = RustVec<UInt8>(try request.serializedData())
            let res: RustVec<UInt8> = try await rustClient.publish(authToken.intoRustString(), req)
            return try PublishResponse(serializedData: Data(res))
        } catch let error as RustString {
            throw ApiClientError.publishError(error.toString())
        }
    }

	@discardableResult func publish(envelopes: [Envelope]) async throws -> PublishResponse {
        return try await publish(request: PublishRequest.with {
            $0.envelopes = envelopes
        })
	}
}
