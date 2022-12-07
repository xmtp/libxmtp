//
//  ConversationV1.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

public struct ConversationV1 {
	var client: Client
	var peerAddress: String
	var sentAt: Date

	var topic: Topic {
		Topic.directMessageV1(client.address, peerAddress)
	}

	func send(content: String, options _: SendOptions? = nil) async throws {
		guard let contact = try await client.getUserContact(peerAddress: peerAddress) else {
			throw ContactBundleError.notFound
		}

		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: content)
		let recipient = try contact.toPublicKeyBundle()

		if !recipient.identityKey.hasSignature {
			fatalError("no signature for id key")
		}

		let message = try MessageV1.encode(
			sender: client.privateKeyBundleV1,
			recipient: recipient,
			message: try encodedContent.serializedData(),
			timestamp: Date()
		)

		try await client.publish(envelopes: [
			Envelope(
				topic: .directMessageV1(client.address, peerAddress),
				timestamp: Date(),
				message: try Message(v1: message).serializedData()
			),
		])
	}

	public func streamMessages() -> AsyncThrowingStream<DecodedMessage, Error> {
		AsyncThrowingStream { continuation in
			Task {
				for try await envelope in client.subscribe(topics: [topic.description]) {
					let decoded = try decode(envelope: envelope)
					continuation.yield(decoded)
				}
			}
		}
	}

	func messages() async throws -> [DecodedMessage] {
		let envelopes = try await client.apiClient.query(topics: [
			.directMessageV1(client.address, peerAddress),
		]).envelopes

		return envelopes.compactMap { envelope in
			do {
				return try decode(envelope: envelope)
			} catch {
				print("ERROR DECODING CONVO V1 MESSAGE: \(error)")
				return nil
			}
		}
	}

	private func decode(envelope: Envelope) throws -> DecodedMessage {
		let message = try Message(serializedData: envelope.message)
		let decrypted = try message.v1.decrypt(with: client.privateKeyBundleV1)

		let encodedMessage = try EncodedContent(serializedData: decrypted)
		let decoder = TextCodec()
		let decoded = try decoder.decode(content: encodedMessage)

		let header = try message.v1.header

		return DecodedMessage(
			body: decoded,
			senderAddress: try header.sender.walletAddress,
			sent: message.v1.sentAt
		)
	}
}
