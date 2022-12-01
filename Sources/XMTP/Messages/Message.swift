//
//  Message.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import XMTPProto

typealias Message = Xmtp_MessageContents_Message

enum MessageVersion: String, RawRepresentable {
	case v1,
	     v2
}

extension Message {
	init(v1: MessageV1) {
		self.init()
		self.v1 = v1
	}
}
