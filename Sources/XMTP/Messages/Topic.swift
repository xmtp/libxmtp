//
//  Topic.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import XMTPProto

enum Topic {
	case userPrivateStoreKeyBundle(String),
	     contact(String),
	     userIntro(String),
	     userInvite(String),
	     directMessageV1(String, String),
	     directMessageV2(String)

	var description: String {
		switch self {
		case let .userPrivateStoreKeyBundle(address):
			return wrap("privatestore-\(address)/key_bundle")
		case let .contact(address):
			return wrap("contact-\(address)")
		case let .userIntro(address):
			return wrap("intro-\(address)")
		case let .userInvite(address):
			return wrap("invite-\(address)")
		case let .directMessageV1(address1, address2):
			let addresses = [address1, address2].sorted().joined(separator: "-")
			return wrap("dm-\(addresses)")
		case let .directMessageV2(randomString):
			return wrap("m-\(randomString)")
		}
	}

	private func wrap(_ value: String) -> String {
		"/xmtp/0/\(value)/proto"
	}
}
