//
//  Environment.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

/// Contains hosts an `ApiClient` can connect to
public enum XMTPEnvironment: String, Sendable {
	case dev = "grpc.dev.xmtp.network:443",
	     production = "grpc.production.xmtp.network:443",
	     local = "localhost:5556"

	var url: String {
		switch self {
		case .dev:
			return "https://\(rawValue)"
		case .production:
			return "https://\(rawValue)"
		case .local:
			return "http://\(rawValue)"
		}
	}

	public var isSecure: Bool {
		url.starts(with: "https")
	}
}
