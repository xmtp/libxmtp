//
//  XMTPDebugInformation.swift
//  XMTPiOS
//
//  Created by Cameron Voell on 5/23/25.
//

import Foundation
import LibXMTP

public class XMTPDebugInformation {
    private let client: Client
    private let ffiClient: FfiXmtpClient
    
    public init(client: Client, ffiClient: FfiXmtpClient) {
        self.client = client
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
    
    public func uploadDebugInformation(serverUrl: String? = nil) async throws -> String {
        let url = serverUrl ?? client.environment.getHistorySyncUrl()
        return try await ffiClient.uploadDebugArchive(serverUrl: url)
    }
}

public class ApiStats {
    private let apiStats: FfiApiStats
    
    public init(apiStats: FfiApiStats) {
        self.apiStats = apiStats
    }
    
    public var uploadKeyPackage: Int64 {
        Int64(apiStats.uploadKeyPackage)
    }
    
    public var fetchKeyPackage: Int64 {
        Int64(apiStats.fetchKeyPackage)
    }
    
    public var sendGroupMessages: Int64 {
        Int64(apiStats.sendGroupMessages)
    }
    
    public var sendWelcomeMessages: Int64 {
        Int64(apiStats.sendWelcomeMessages)
    }
    
    public var queryGroupMessages: Int64 {
        Int64(apiStats.queryGroupMessages)
    }
    
    public var queryWelcomeMessages: Int64 {
        Int64(apiStats.queryWelcomeMessages)
    }
    
    public var subscribeMessages: Int64 {
        Int64(apiStats.subscribeMessages)
    }
    
    public var subscribeWelcomes: Int64 {
        Int64(apiStats.subscribeWelcomes)
    }
}

public class IdentityStats {
    private let identityStats: FfiIdentityStats
    
    public init(identityStats: FfiIdentityStats) {
        self.identityStats = identityStats
    }
    
    public var publishIdentityUpdate: Int64 {
        Int64(identityStats.publishIdentityUpdate)
    }
    
    public var getIdentityUpdatesV2: Int64 {
        Int64(identityStats.getIdentityUpdatesV2)
    }
    
    public var getInboxIds: Int64 {
        Int64(identityStats.getInboxIds)
    }
    
    public var verifySmartContractWalletSignature: Int64 {
        Int64(identityStats.verifySmartContractWalletSignature)
    }
}



