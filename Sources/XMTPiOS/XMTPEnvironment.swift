//
//  XMTPEnvironment.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

public func getLocalAddressFromEnvironment() -> String? {
	ProcessInfo.processInfo.environment["XMTP_NODE_ADDRESS"]
}

public func getHistorySyncUrlFromEnvironment() -> String? {
	ProcessInfo.processInfo.environment["XMTP_HISTORY_SERVER_ADDRESS"]
}

/// Contains hosts an `ApiClient` can connect to
public enum XMTPEnvironment: String, Sendable {
	case dev = "grpc.dev.xmtp.network"
	case production = "grpc.production.xmtp.network"
	case local = "localhost"

	// Optional override for the local environment
	public static var customLocalAddress: String?
	public static var customHistorySyncUrl: String?

	var address: String {
		switch self {
		case .local:
			return XMTPEnvironment.customLocalAddress ?? rawValue
		default:
			return rawValue
		}
	}

	var legacyRawValue: String {
		switch self {
		case .local:
			return "localhost:5556"
		case .dev:
			return "grpc.dev.xmtp.network:443"
		case .production:
			return "grpc.production.xmtp.network:443"
		}
	}

	var url: String {
		switch self {
		case .dev, .production:
			return "https://\(address):443"
		case .local:
			if address.starts(with: "http://") || address.starts(with: "https://") {
				return address
			}
			return "http://\(address):5556"
		}
	}

	public var isSecure: Bool {
		url.starts(with: "https")
	}

	public func getHistorySyncUrl() -> String {
		switch self {
		case .production:
			return "https://message-history.production.ephemera.network"
		case .local:
			return XMTPEnvironment.customHistorySyncUrl ?? "http://localhost:5558"
		case .dev:
			return "https://message-history.dev.ephemera.network"
		}
	}
}
