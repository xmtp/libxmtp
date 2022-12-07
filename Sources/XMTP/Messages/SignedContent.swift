//
//  SignedContent.swift
//
//
//  Created by Pat Nakajima on 12/5/22.
//

import Foundation
import XMTPProto

typealias SignedContent = Xmtp_MessageContents_SignedContent

extension SignedContent {
	init(payload: Data, sender: SignedPublicKeyBundle, signature: Signature) {
		self.init()
		self.payload = payload
		self.sender = sender
		self.signature = signature
	}
}
