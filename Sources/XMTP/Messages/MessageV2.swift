//
//  MessageV2.swift
//
//
//  Created by Pat Nakajima on 12/5/22.
//

import CryptoKit
import Foundation
import XMTPRust

typealias MessageV2 = Xmtp_MessageContents_MessageV2

enum MessageV2Error: Error {
	case invalidSignature, decodeError(String)
}

extension MessageV2 {
	init(headerBytes: Data, ciphertext: CipherText) {
		self.init()
		self.headerBytes = headerBytes
		self.ciphertext = ciphertext
	}

	static func decrypt(_ id: String, _ topic: String, _ message: MessageV2, keyMaterial: Data, client: Client) throws -> DecryptedMessage {
		let decrypted = try Crypto.decrypt(keyMaterial, message.ciphertext, additionalData: message.headerBytes)
		let signed = try SignedContent(serializedData: decrypted)

		guard signed.sender.hasPreKey, signed.sender.hasIdentityKey else {
			throw MessageV2Error.decodeError("missing sender pre-key or identity key")
		}

		let senderPreKey = try PublicKey(signed.sender.preKey)
		let senderIdentityKey = try PublicKey(signed.sender.identityKey)

		// This is a bit confusing since we're passing keyBytes as the digest instead of a SHA256 hash.
		// That's because our underlying crypto library always SHA256's whatever data is sent to it for this.
		if !(try senderPreKey.signature.verify(signedBy: senderIdentityKey, digest: signed.sender.preKey.keyBytes)) {
			throw MessageV2Error.decodeError("pre-key not signed by identity key")
		}

		// Verify content signature
		let key = try PublicKey.with { key in
			key.secp256K1Uncompressed.bytes = try KeyUtilx.recoverPublicKeySHA256(from: signed.signature.rawData, message: Data(message.headerBytes + signed.payload))
		}

		if key.walletAddress != (try PublicKey(signed.sender.preKey).walletAddress) {
			throw MessageV2Error.invalidSignature
		}

		let encodedMessage = try EncodedContent(serializedData: signed.payload)
		let header = try MessageHeaderV2(serializedData: message.headerBytes)

		return DecryptedMessage(
			id: id,
			encodedContent: encodedMessage,
			senderAddress: try signed.sender.walletAddress,
			sentAt: Date(timeIntervalSince1970: Double(header.createdNs / 1_000_000) / 1000),
			topic: topic
		)
	}

	static func decode(_ id: String, _ topic: String, _ message: MessageV2, keyMaterial: Data, client: Client) throws -> DecodedMessage {
		do {
			let decryptedMessage = try decrypt(id, topic, message, keyMaterial: keyMaterial, client: client)

			return DecodedMessage(
				id: id,
				client: client,
				topic: decryptedMessage.topic,
				encodedContent: decryptedMessage.encodedContent,
				senderAddress: decryptedMessage.senderAddress,
				sent: decryptedMessage.sentAt
			)
		} catch {
			print("ERROR DECODING: \(error)")
			throw error
		}
	}

	static func encode(client: Client, content encodedContent: EncodedContent, topic: String, keyMaterial: Data) async throws -> MessageV2 {
		let payload = try encodedContent.serializedData()

		let date = Date()
		let header = MessageHeaderV2(topic: topic, created: date)
		let headerBytes = try header.serializedData()

		let digest = SHA256.hash(data: headerBytes + payload)
		let preKey = client.keys.preKeys[0]
		let signature = try await preKey.sign(Data(digest))

		let bundle = client.privateKeyBundleV1.toV2().getPublicKeyBundle()

		let signedContent = SignedContent(payload: payload, sender: bundle, signature: signature)
		let signedBytes = try signedContent.serializedData()

		let ciphertext = try Crypto.encrypt(keyMaterial, signedBytes, additionalData: headerBytes)

		return MessageV2(
			headerBytes: headerBytes,
			ciphertext: ciphertext
		)
	}
}
