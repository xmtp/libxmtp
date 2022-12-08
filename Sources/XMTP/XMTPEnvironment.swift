//
//  Environment.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

/// Contains hosts an `ApiClient` can connect to
public enum XMTPEnvironment: String {
	case dev = "dev.xmtp.network",
	     production = "production.xmtp.network",
	     local = "localhost"
}
