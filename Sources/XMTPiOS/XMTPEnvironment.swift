//
//  Environment.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

/// Contains hosts an `ApiClient` can connect to
public enum XMTPEnvironment: String, Sendable {
	case dev = "grpc.dev.xmtp.network"
	case production = "grpc.production.xmtp.network"
	case local = "localhost"

	// Optional override for the local environment
	public static var customLocalAddress: String?

	var address: String {
		switch self {
		case .local:
			return XMTPEnvironment.customLocalAddress ?? rawValue
		default:
			return rawValue
		}
	}

	var url: String {
		switch self {
		case .dev, .production:
			return "https://\(address):443"
		case .local:
			return "http://\(address):5556"
		}
	}

	public var isSecure: Bool {
		url.starts(with: "https")
	}
}
