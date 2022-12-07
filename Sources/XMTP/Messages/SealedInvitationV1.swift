//
//  SealedInvitationV1.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

typealias SealedInvitationV1 = Xmtp_MessageContents_SealedInvitationV1

extension SealedInvitationV1 {
	init(headerBytes: Data, ciphtertext: CipherText, header _: SealedInvitationHeaderV1? = nil) {
		self.init()
		self.headerBytes = headerBytes
		ciphertext = ciphtertext
	}

	var header: SealedInvitationHeaderV1 {
		do {
			return try SealedInvitationHeaderV1(serializedData: headerBytes)
		} catch {
			return SealedInvitationHeaderV1()
		}
	}

	func getInvitation(viewer: PrivateKeyBundleV2) throws -> InvitationV1 {
		let header = header

		var secret: Data

		if !header.sender.identityKey.hasSignature {
			throw SealedInvitationError.noSignature
		}

		if viewer.identityKey.matches(header.sender.identityKey) {
			secret = try viewer.sharedSecret(peer: header.recipient, myPreKey: header.sender.preKey, isRecipient: false)
		} else {
			secret = try viewer.sharedSecret(peer: header.sender, myPreKey: header.recipient.preKey, isRecipient: true)
		}

		let decryptedBytes = try Crypto.decrypt(secret, ciphertext, additionalData: headerBytes)
		let invitation = try InvitationV1(serializedData: decryptedBytes)

		return invitation
	}
}
