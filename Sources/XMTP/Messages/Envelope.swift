//
//  Envelope.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

public typealias Envelope = Xmtp_MessageApi_V1_Envelope

extension Envelope {
	init(topic: Topic, timestamp: Date, message: Data) {
		self.init()
		contentTopic = topic.description
		timestampNs = UInt64(timestamp.millisecondsSinceEpoch * 1_000_000)
		self.message = message
	}

	init(topic: String, timestamp: Date, message: Data) {
		self.init()
		contentTopic = topic
		timestampNs = UInt64(timestamp.millisecondsSinceEpoch * 1_000_000)
		self.message = message
	}
}
