//
//  SealedInvitationHeaderV1.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import XMTPProto

typealias SealedInvitationHeaderV1 = Xmtp_MessageContents_SealedInvitationHeaderV1

extension SealedInvitationHeaderV1 {
	init(sender: SignedPublicKeyBundle, recipient: SignedPublicKeyBundle, createdNs: UInt64) {
		self.init()
		self.sender = sender
		self.recipient = recipient
		self.createdNs = createdNs
	}
}
