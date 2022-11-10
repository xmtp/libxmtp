//
//  AuthData.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import XMTPProto

typealias AuthData = Xmtp_MessageApi_V1_AuthData

extension AuthData {
	init(walletAddress: String, timestamp: Date? = nil) {
		self.init()
		walletAddr = walletAddress

		let timestamp = timestamp ?? Date()
		createdNs = UInt64(timestamp.millisecondsSinceEpoch * 1_000_000)
	}
}
