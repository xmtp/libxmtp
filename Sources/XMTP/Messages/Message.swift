//
//  Message.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import XMTPProto

/// Handles encryption/decryption for communicating data in conversations
public typealias Message = Xmtp_MessageContents_Message

public enum MessageVersion: String, RawRepresentable {
	case v1,
	     v2
}

extension Message {
	init(v1: MessageV1) {
		self.init()
		self.v1 = v1
	}

	init(v2: MessageV2) {
		self.init()
		self.v2 = v2
	}
}
