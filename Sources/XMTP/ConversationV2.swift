//
//  ConversationV2.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

struct SendOptions {}

struct ConversationV2 {
	var topic: String
	var keyMaterial: Data // MUST be kept secret
	var context: InvitationV1.Context
	var peerAddress: String
	private var client: Client
	private var header: SealedInvitationHeaderV1

	static func create(client: Client, invitation: InvitationV1, header: SealedInvitationHeaderV1) throws -> ConversationV2 {
		let myKeys = client.keys.getPublicKeyBundle()

		let peer = try myKeys.walletAddress == (try header.sender.walletAddress) ? header.recipient : header.sender
		let peerAddress = try peer.walletAddress

		let keyMaterial = Data(invitation.aes256GcmHkdfSha256.keyMaterial.bytes)

		return ConversationV2(
			topic: invitation.topic,
			keyMaterial: keyMaterial,
			context: invitation.context,
			peerAddress: peerAddress,
			client: client,
			header: header
		)
	}

	init(topic: String, keyMaterial: Data, context: InvitationV1.Context, peerAddress: String, client: Client, header: SealedInvitationHeaderV1) {
		self.topic = topic
		self.keyMaterial = keyMaterial
		self.context = context
		self.peerAddress = peerAddress
		self.client = client
		self.header = header
	}

	func messages() async throws -> [DecodedMessage] {
		let envelopes = try await client.apiClient.query(topics: [topic]).envelopes

		return envelopes.compactMap { envelope in
			do {
				let message = try Message(serializedData: envelope.message)
				let decrypted = try message.v1.decrypt(with: client.privateKeyBundleV1)
				let encodedMessage = try EncodedContent(serializedData: decrypted)
				let decoder = TextCodec()
				let decoded = try decoder.decode(content: encodedMessage)

				return DecodedMessage(body: decoded)
			} catch {
				print("Error decoding envelope \(error)")
				return nil
			}
		}
	}

	// TODO: more types of content
	func send(content: String, options _: SendOptions? = nil) async throws {
		guard let contact = try await client.getUserContact(peerAddress: peerAddress) else {
			throw ContactBundleError.notFound
		}

		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: content)

		let signedPublicKeyBundle = try contact.toSignedPublicKeyBundle()
		let recipient = try PublicKeyBundle(signedPublicKeyBundle)

		let message = try MessageV1.encode(
			sender: client.privateKeyBundleV1,
			recipient: recipient,
			message: try encodedContent.serializedData(),
			timestamp: Date()
		)

		try await client.publish(envelopes: [
			Envelope(topic: .userIntro(recipient.walletAddress), timestamp: Date(), message: try message.serializedData()),
			Envelope(topic: .userIntro(client.address), timestamp: Date(), message: try message.serializedData()),
			Envelope(topic: topic, timestamp: Date(), message: try Message(v1: message).serializedData()),
		])
	}
}
