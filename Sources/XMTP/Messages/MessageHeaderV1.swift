//
//  MessageHeaderV1.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import Foundation
import XMTPProto

typealias MessageHeaderV1 = Xmtp_MessageContents_MessageHeaderV1

extension MessageHeaderV1 {
	init(sender: PublicKeyBundle, recipient: PublicKeyBundle, timestamp: UInt64) {
		self.init()
		self.sender = sender
		self.recipient = recipient
		self.timestamp = timestamp
	}
}
