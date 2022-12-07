//
//  SealedInvitation.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

typealias SealedInvitation = Xmtp_MessageContents_SealedInvitation

enum SealedInvitationError: Error {
	case noSignature
}

extension SealedInvitation {
	static func createV1(sender: PrivateKeyBundleV2, recipient: SignedPublicKeyBundle, created: Date, invitation: InvitationV1) throws -> SealedInvitation {
		let header = SealedInvitationHeaderV1(
			sender: sender.getPublicKeyBundle(),
			recipient: recipient,
			createdNs: UInt64(created.millisecondsSinceEpoch * 1_000_000)
		)

		let secret = try sender.sharedSecret(peer: recipient, myPreKey: sender.preKeys[0].publicKey, isRecipient: false)

		let headerBytes = try header.serializedData()
		let invitationBytes = try invitation.serializedData()

		let ciphertext = try Crypto.encrypt(secret, invitationBytes, additionalData: headerBytes)

		return SealedInvitation(headerBytes: headerBytes, ciphertext: ciphertext)
	}

	init(headerBytes: Data, ciphertext: CipherText) {
		self.init()
		v1.headerBytes = headerBytes
		v1.ciphertext = ciphertext
	}

	func involves(_ contact: ContactBundle) -> Bool {
		do {
			let contactSignedPublicKeyBundle = try contact.toSignedPublicKeyBundle()

			return v1.header.recipient.equals(contactSignedPublicKeyBundle) || v1.header.sender.equals(contactSignedPublicKeyBundle)
		} catch {
			return false
		}
	}
}
