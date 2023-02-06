//
//  ConversationV1.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import CryptoKit
import Foundation

// Save the non-client parts for a v1 conversation
public struct ConversationV1Container: Codable {
	var peerAddress: String
	var sentAt: Date

	func decode(with client: Client) -> ConversationV1 {
		ConversationV1(client: client, peerAddress: peerAddress, sentAt: sentAt)
	}
}

/// Handles legacy message conversations.
public struct ConversationV1 {
	public var client: Client
	public var peerAddress: String
	public var sentAt: Date

	public init(client: Client, peerAddress: String, sentAt: Date) {
		self.client = client
		self.peerAddress = peerAddress
		self.sentAt = sentAt
	}

	public var encodedContainer: ConversationV1Container {
		ConversationV1Container(peerAddress: peerAddress, sentAt: sentAt)
	}

	var topic: Topic {
		Topic.directMessageV1(client.address, peerAddress)
	}

	func send(content: String, options: SendOptions? = nil) async throws {
		try await send(content: content, options: options, sentAt: nil)
	}

	internal func send(content: String, options: SendOptions? = nil, sentAt: Date? = nil) async throws {
		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: content)

		try await send(content: encodedContent, options: options, sentAt: sentAt)
	}

	func send<T>(content: T, options: SendOptions? = nil) async throws {
		let codec = Client.codecRegistry.find(for: options?.contentType)

		func encode<Codec: ContentCodec>(codec: Codec, content: Any) throws -> EncodedContent {
			if let content = content as? Codec.T {
				return try codec.encode(content: content)
			} else {
				throw CodecError.invalidContent
			}
		}

		let content = content as T
		var encoded = try encode(codec: codec, content: content)
		encoded.fallback = options?.contentFallback ?? ""
		try await send(content: encoded, options: options)
	}

	internal func send(content encodedContent: EncodedContent, options: SendOptions? = nil, sentAt: Date? = nil) async throws {
		guard let contact = try await client.contacts.find(peerAddress) else {
			throw ContactBundleError.notFound
		}

		var encodedContent = encodedContent

		if let compression = options?.compression {
			encodedContent = try encodedContent.compress(compression)
		}

		let recipient = try contact.toPublicKeyBundle()

		if !recipient.identityKey.hasSignature {
			fatalError("no signature for id key")
		}

		let date = sentAt ?? Date()

		let message = try MessageV1.encode(
			sender: client.privateKeyBundleV1,
			recipient: recipient,
			message: try encodedContent.serializedData(),
			timestamp: date
		)

		var envelopes = [
			Envelope(
				topic: .directMessageV1(client.address, peerAddress),
				timestamp: date,
				message: try Message(v1: message).serializedData()
			),
		]

		if client.contacts.needsIntroduction(peerAddress) {
			envelopes.append(contentsOf: [
				Envelope(
					topic: .userIntro(peerAddress),
					timestamp: date,
					message: try Message(v1: message).serializedData()
				),
				Envelope(
					topic: .userIntro(client.address),
					timestamp: date,
					message: try Message(v1: message).serializedData()
				),
			])

			client.contacts.hasIntroduced[peerAddress] = true
		}

		try await client.publish(envelopes: envelopes)
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

	func messages(limit: Int? = nil, before: Date? = nil, after: Date? = nil) async throws -> [DecodedMessage] {
		let pagination = Pagination(limit: limit, startTime: before, endTime: after)

		let envelopes = try await client.apiClient.query(topics: [
			.directMessageV1(client.address, peerAddress),
		], pagination: pagination).envelopes

		return envelopes.compactMap { envelope in
			do {
				return try decode(envelope: envelope)
			} catch {
				print("ERROR DECODING CONVO V1 MESSAGE: \(error)")
				return nil
			}
		}
	}

	public func decode(envelope: Envelope) throws -> DecodedMessage {
		let message = try Message(serializedData: envelope.message)
		let decrypted = try message.v1.decrypt(with: client.privateKeyBundleV1)

		let encodedMessage = try EncodedContent(serializedData: decrypted)
		let header = try message.v1.header

		var decoded = DecodedMessage(
			encodedContent: encodedMessage,
			senderAddress: header.sender.walletAddress,
			sent: message.v1.sentAt
		)

		decoded.id = generateID(from: envelope)

		return decoded
	}

	private func generateID(from envelope: Envelope) -> String {
		Data(SHA256.hash(data: envelope.message)).toHex
	}
}
