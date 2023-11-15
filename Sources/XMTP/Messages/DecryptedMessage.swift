//
//  DecryptedMessage.swift
//
//
//  Created by Pat Nakajima on 11/14/23.
//

import Foundation

public struct DecryptedMessage {
	var id: String
	var encodedContent: EncodedContent
	var senderAddress: String
	var sentAt: Date
	var topic: String = ""
}
