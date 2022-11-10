//
//  Topic.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import XMTPProto

enum Topic: CustomStringConvertible {
	case userPrivateStoreKeyBundle(String)

	var description: String {
		switch self {
		case let .userPrivateStoreKeyBundle(address):
			return wrap("privatestore-\(address)")
		}
	}

	private func wrap(_ value: String) -> String {
		"/xmtp/0/\(value)/proto"
	}
}
