//
//  MessageHeaderV2.swift
//
//
//  Created by Pat Nakajima on 12/5/22.
//

import Foundation
import XMTPProto

typealias MessageHeaderV2 = Xmtp_MessageContents_MessageHeaderV2

extension MessageHeaderV2 {
	init(topic: String, created: Date) {
		self.init()
		self.topic = topic
		createdNs = UInt64(created.millisecondsSinceEpoch * 1_000_000)
	}
}
