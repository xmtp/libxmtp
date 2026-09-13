//
//  XMTPDebugInformation.swift
//  XMTPiOS
//
//  Created by Cameron Voell on 5/23/25.
//

import Foundation

public class XMTPDebugInformation {
	private let ffiClient: FfiXmtpClient

	public init(ffiClient: FfiXmtpClient) {
		self.ffiClient = ffiClient
	}

	public var apiStatistics: ApiStats {
		ApiStats(apiStats: ffiClient.apiStatistics())
	}

	public var identityStatistics: IdentityStats {
		IdentityStats(identityStats: ffiClient.apiIdentityStatistics())
	}

	public var aggregateStatistics: String {
		ffiClient.apiAggregateStatistics()
	}

	public func clearAllStatistics() {
		ffiClient.clearAllStatistics()
	}

	@available(*, deprecated, message: "uploadDebugInformation has been removed from libxmtp")
	public func uploadDebugInformation(serverUrl _: String? = nil) async throws -> String {
		// uploadDebugArchive has been removed from FFI
		throw ClientError.creationError("uploadDebugInformation is no longer available")
	}
}

public class ApiStats {
	private let apiStats: FfiApiStats

	public init(apiStats: FfiApiStats) {
		self.apiStats = apiStats
	}

	public var publish: Int64 {
		Int64(apiStats.publish)
	}

	public var query: Int64 {
		Int64(apiStats.query)
	}

	public var queryNewest: Int64 {
		Int64(apiStats.queryNewest)
	}

	public var subscribe: Int64 {
		Int64(apiStats.subscribe)
	}

	public var subscribeStatic: Int64 {
		Int64(apiStats.subscribeStatic)
	}
}

public class IdentityStats {
	private let identityStats: FfiIdentityStats

	public init(identityStats: FfiIdentityStats) {
		self.identityStats = identityStats
	}

	public var getInboxIds: Int64 {
		Int64(identityStats.getInboxIds)
	}

	public var verifySmartContractWalletSignatures: Int64 {
		Int64(identityStats.verifySmartContractWalletSignatures)
	}
}
