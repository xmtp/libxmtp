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

	func prepareMessage(encodedContent: EncodedContent, options: SendOptions?) async throws -> PreparedMessage {
		guard let contact = try await client.contacts.find(peerAddress) else {
			throw ContactBundleError.notFound
		}

		let recipient = try contact.toPublicKeyBundle()

		if !recipient.identityKey.hasSignature {
			fatalError("no signature for id key")
		}

		let date = sentAt

		let message = try MessageV1.encode(
			sender: client.privateKeyBundleV1,
			recipient: recipient,
			message: try encodedContent.serializedData(),
			timestamp: date
		)

		let isEphemeral: Bool
		if let options, options.ephemeral {
			isEphemeral = true
		} else {
			isEphemeral = false
		}
        let msg = try Message(v1: message).serializedData()
		let messageEnvelope = Envelope(
			topic: isEphemeral ? ephemeralTopic : topic.description,
			timestamp: date,
			message: msg
		)
        var envelopes = [messageEnvelope]
        if (await client.contacts.needsIntroduction(peerAddress)) && !isEphemeral {
            envelopes.append(contentsOf: [
                Envelope(
                    topic: .userIntro(peerAddress),
                    timestamp: date,
                    message: msg
                ),
                Envelope(
                    topic: .userIntro(client.address),
                    timestamp: date,
                    message: msg
                ),
            ])

					await client.contacts.markIntroduced(peerAddress, true)
        }

		return PreparedMessage(envelopes: envelopes)
	}

	func prepareMessage<T>(content: T, options: SendOptions?) async throws -> PreparedMessage {
		let codec = client.codecRegistry.find(for: options?.contentType)

		func encode<Codec: ContentCodec>(codec: Codec, content: Any) throws -> EncodedContent {
			if let content = content as? Codec.T {
				return try codec.encode(content: content, client: client)
			} else {
				throw CodecError.invalidContent
			}
		}

		let content = content as T
		var encoded = try encode(codec: codec, content: content)
        
        func fallback<Codec: ContentCodec>(codec: Codec, content: Any) throws -> String? {
            if let content = content as? Codec.T {
                return try codec.fallback(content: content)
            } else {
                throw CodecError.invalidContent
            }
        }
        
        if let fallback = try fallback(codec: codec, content: content) {
            encoded.fallback = fallback
        }
        
		if let compression = options?.compression {
			encoded = try encoded.compress(compression)
		}

		return try await prepareMessage(encodedContent: encoded, options: options)
	}

	@discardableResult func send(content: String, options: SendOptions? = nil) async throws -> String {
		return try await send(content: content, options: options, sentAt: nil)
	}

	@discardableResult internal func send(content: String, options: SendOptions? = nil, sentAt _: Date? = nil) async throws -> String {
		let preparedMessage = try await prepareMessage(content: content, options: options)
        return try await send(prepared: preparedMessage)
	}

	@discardableResult func send(encodedContent: EncodedContent, options: SendOptions?) async throws -> String {
		let preparedMessage = try await prepareMessage(encodedContent: encodedContent, options: options)
        return try await send(prepared: preparedMessage)
	}

    @discardableResult func send(prepared: PreparedMessage) async throws -> String {
        try await client.publish(envelopes: prepared.envelopes)
        if((await client.contacts.consentList.state(address: peerAddress)) == .unknown) {
            try await client.contacts.allow(addresses: [peerAddress])
        }
        return prepared.messageID
    }

	func send<T>(content: T, options: SendOptions? = nil) async throws -> String {
		let preparedMessage = try await prepareMessage(content: content, options: options)
        return try await send(prepared: preparedMessage)
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

	var ephemeralTopic: String {
		topic.description.replacingOccurrences(of: "/xmtp/0/dm-", with: "/xmtp/0/dmE-")
	}

	public func streamEphemeral() -> AsyncThrowingStream<Envelope, Error> {
		AsyncThrowingStream { continuation in
			Task {
				do {
					for try await envelope in client.subscribe(topics: [ephemeralTopic]) {
						continuation.yield(envelope)
					}
				} catch {
					continuation.finish(throwing: error)
				}
			}
		}
	}

	func decryptedMessages(limit: Int? = nil, before: Date? = nil, after: Date? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecryptedMessage] {
		let pagination = Pagination(limit: limit, before: before, after: after, direction: direction)

		let envelopes = try await client.apiClient.envelopes(
						topic: Topic.directMessageV1(client.address, peerAddress).description,
			pagination: pagination
		)

		return try envelopes.map { try decrypt(envelope: $0) }
	}

	func messages(limit: Int? = nil, before: Date? = nil, after: Date? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecodedMessage] {
		let pagination = Pagination(limit: limit, before: before, after: after, direction: direction)

		let envelopes = try await client.apiClient.envelopes(
            topic: Topic.directMessageV1(client.address, peerAddress).description,
			pagination: pagination
		)

		return envelopes.compactMap { envelope in
			do {
				return try decode(envelope: envelope)
			} catch {
				print("ERROR DECODING CONVO V1 MESSAGE: \(error)")
				return nil
			}
		}
	}

	func decrypt(envelope: Envelope) throws -> DecryptedMessage {
		let message = try Message(serializedData: envelope.message)
		let decrypted = try message.v1.decrypt(with: client.privateKeyBundleV1)

		let encodedMessage = try EncodedContent(serializedData: decrypted)
		let header = try message.v1.header

		return DecryptedMessage(id: generateID(from: envelope), encodedContent: encodedMessage, senderAddress: header.sender.walletAddress, sentAt: message.v1.sentAt)
	}

	public func decode(envelope: Envelope) throws -> DecodedMessage {
		let decryptedMessage = try decrypt(envelope: envelope)

		var decoded = DecodedMessage(
			client: client,
			topic: envelope.contentTopic,
			encodedContent: decryptedMessage.encodedContent,
			senderAddress: decryptedMessage.senderAddress,
			sent: decryptedMessage.sentAt
		)

		decoded.id = generateID(from: envelope)

		return decoded
	}

	private func generateID(from envelope: Envelope) -> String {
		Data(SHA256.hash(data: envelope.message)).toHex
	}
}
