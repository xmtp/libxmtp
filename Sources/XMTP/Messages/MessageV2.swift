//
//  MessageV2.swift
//
//
//  Created by Pat Nakajima on 12/5/22.
//

import CryptoKit
import Foundation
import XMTPProto

typealias MessageV2 = Xmtp_MessageContents_MessageV2

extension MessageV2 {
	init(headerBytes: Data, ciphertext: CipherText) {
		self.init()
		self.headerBytes = headerBytes
		self.ciphertext = ciphertext
	}

	static func decode(_ message: MessageV2, keyMaterial: Data) throws -> DecodedMessage {
		do {
			let decrypted = try Crypto.decrypt(keyMaterial, message.ciphertext, additionalData: message.headerBytes)
			let signed = try SignedContent(serializedData: decrypted)
			let encodedMessage = try EncodedContent(serializedData: signed.payload)
			let decoder = TextCodec()
			let decoded = try decoder.decode(content: encodedMessage)

			let header = try MessageHeaderV2(serializedData: message.headerBytes)

			return DecodedMessage(
				body: decoded,
				senderAddress: try signed.sender.walletAddress,
				sent: Date(timeIntervalSince1970: Double(header.createdNs / 1_000_000) / 1000)
			)
		} catch {
			print("ERROR DECODING: \(error)")
			throw error
		}
	}

	static func encode(client: Client, content: String, topic: String, keyMaterial: Data) async throws -> MessageV2 {
		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: content)
		let payload = try encodedContent.serializedData()

		let date = Date()
		let header = MessageHeaderV2(topic: topic, created: date)
		let headerBytes = try header.serializedData()

		let digest = SHA256.hash(data: headerBytes + payload)
		let preKey = client.keys.preKeys[0]
		let signature = try await preKey.sign(Data(digest))

		let bundle = try client.privateKeyBundleV1.toV2().getPublicKeyBundle()

		let signedContent = SignedContent(payload: payload, sender: bundle, signature: signature)
		let signedBytes = try signedContent.serializedData()

		let ciphertext = try Crypto.encrypt(keyMaterial, signedBytes, additionalData: headerBytes)

		return MessageV2(
			headerBytes: headerBytes,
			ciphertext: ciphertext
		)
	}
}
