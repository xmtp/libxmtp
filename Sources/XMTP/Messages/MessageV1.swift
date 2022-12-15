//
//  MessageV1.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

typealias MessageV1 = Xmtp_MessageContents_MessageV1

enum MessageV1Error: Error {
	case cannotDecodeFromBytes
}

extension MessageV1 {
	static func encode(sender: PrivateKeyBundleV1, recipient: PublicKeyBundle, message: Data, timestamp: Date) throws -> MessageV1 {
		let secret = try sender.sharedSecret(
			peer: recipient,
			myPreKey: sender.preKeys[0].publicKey,
			isRecipient: false
		)

		let header = MessageHeaderV1(
			sender: sender.toPublicKeyBundle(),
			recipient: recipient,
			timestamp: UInt64(timestamp.millisecondsSinceEpoch)
		)

		let headerBytes = try header.serializedData()
		let ciphertext = try Crypto.encrypt(secret, message, additionalData: headerBytes)

		return MessageV1(headerBytes: headerBytes, ciphertext: ciphertext)
	}

	static func fromBytes(_ bytes: Data) throws -> MessageV1 {
		let message = try Message(serializedData: bytes)
		var headerBytes: Data
		var ciphertext: CipherText

		switch message.version {
		case .v1:
			headerBytes = message.v1.headerBytes
			ciphertext = message.v1.ciphertext
		case .v2:
			headerBytes = message.v2.headerBytes
			ciphertext = message.v2.ciphertext
		default:
			throw MessageV1Error.cannotDecodeFromBytes
		}

		return MessageV1(headerBytes: headerBytes, ciphertext: ciphertext)
	}

	init(headerBytes: Data, ciphertext: CipherText) {
		self.init()
		self.headerBytes = headerBytes
		self.ciphertext = ciphertext
	}

	var header: MessageHeaderV1 {
		get throws {
			do {
				return try MessageHeaderV1(serializedData: headerBytes)
			} catch {
				print("Error deserializing MessageHeaderV1 \(error)")
				throw error
			}
		}
	}

	var senderAddress: String? {
		do {
			let senderKey = try header.sender.identityKey.recoverWalletSignerPublicKey()
			return senderKey.walletAddress
		} catch {
			print("Error getting sender address: \(error)")
			return nil
		}
	}

	var sentAt: Date {
		// swiftlint:disable force_try
		try! Date(timeIntervalSince1970: Double(header.timestamp / 1000))
		// swiftlint:enable force_try
	}

	var recipientAddress: String? {
		do {
			let recipientKey = try header.recipient.identityKey.recoverWalletSignerPublicKey()

			return recipientKey.walletAddress
		} catch {
			print("Error getting recipient address: \(error)")
			return nil
		}
	}

	func decrypt(with viewer: PrivateKeyBundleV1) throws -> Data {
		let header = try MessageHeaderV1(serializedData: headerBytes)

		let recipient = header.recipient
		let sender = header.sender

		var secret: Data
		if viewer.walletAddress == sender.walletAddress {
			secret = try viewer.sharedSecret(peer: recipient, myPreKey: sender.preKey, isRecipient: false)
		} else {
			secret = try viewer.sharedSecret(peer: sender, myPreKey: recipient.preKey, isRecipient: true)
		}

		return try Crypto.decrypt(secret, ciphertext, additionalData: headerBytes)
	}
}
