//
//  DecodedMessage.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

public struct DecodedMessage {
	public var body: String
	public var senderAddress: String
	public var sent: Date

	public init(body: String, senderAddress: String, sent: Date) {
		self.body = body
		self.senderAddress = senderAddress
		self.sent = sent
	}
}
