//
//  DecryptedMessage.swift
//
//
//  Created by Pat Nakajima on 11/14/23.
//

import Foundation

public struct DecryptedMessage {
	public var id: String
	public var encodedContent: EncodedContent
	public var senderAddress: String
	public var sentAt: Date
	public var topic: String = ""
}
