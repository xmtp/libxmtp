//
//  Conversation.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation
import LibXMTP

public enum ConversationContainer: Codable {
	case v1(ConversationV1Container), v2(ConversationV2Container)

	public func decode(with client: Client) -> Conversation {
		switch self {
		case let .v1(container):
			return .v1(container.decode(with: client))
		case let .v2(container):
			return .v2(container.decode(with: client))
		}
	}
}

/// Wrapper that provides a common interface between ``ConversationV1`` and ``ConversationV2`` objects.
public enum Conversation: Sendable {
	// TODO: It'd be nice to not have to expose these types as public, maybe we make this a struct with an enum prop instead of just an enum
	case v1(ConversationV1), v2(ConversationV2), group(Group), dm(Dm)

	public enum Version {
		case v1, v2, group, dm
	}
	
	public var id: String {
		get throws {
			switch self {
			case .v1(_):
				throw ConversationError.v1NotSupported("id")
			case .v2(_):
				throw ConversationError.v2NotSupported("id")
			case let .group(group):
				return group.id
			case let .dm(dm):
				return dm.id
			}
		}
	}
	
	public func isCreator() async throws -> Bool {
		switch self {
		case .v1(_):
			throw ConversationError.v1NotSupported("isCreator")
		case .v2(_):
			throw ConversationError.v2NotSupported("isCreator")
		case let .group(group):
			return try group.isCreator()
		case let .dm(dm):
			return try dm.isCreator()
		}
	}
	
	public func members() async throws -> [Member] {
		switch self {
		case .v1(_):
			throw ConversationError.v1NotSupported("members")
		case .v2(_):
			throw ConversationError.v2NotSupported("members")
		case let .group(group):
			return try await group.members
		case let .dm(dm):
			return try await dm.members
		}
	}

	public func consentState() async throws -> ConsentState {
		switch self {
		case .v1(let conversationV1):
			return try await conversationV1.client.contacts.consentList.state(address: peerAddress)
		case .v2(let conversationV2):
			return try await conversationV2.client.contacts.consentList.state(address: peerAddress)
		case let .group(group):
			return try group.consentState()
		case let .dm(dm):
			return try dm.consentState()
		}
	}
	
	public func updateConsentState(state: ConsentState) async throws {
		switch self {
		case .v1(_):
			throw ConversationError.v1NotSupported("updateConsentState use contact.allowAddresses instead")
		case .v2(_):
			throw ConversationError.v2NotSupported("updateConsentState use contact.allowAddresses instead")
		case let .group(group):
			try await group.updateConsentState(state: state)
		case let .dm(dm):
			try await dm.updateConsentState(state: state)
		}
	}
	
	public func sync() async throws {
		switch self {
		case .v1(_):
			throw ConversationError.v1NotSupported("sync")
		case .v2(_):
			throw ConversationError.v2NotSupported("sync")
		case let .group(group):
			try await group.sync()
		case let .dm(dm):
			try await dm.sync()
		}
	}

	public func processMessage(envelopeBytes: Data) async throws -> MessageV3 {
		switch self {
		case .v1(_):
			throw ConversationError.v1NotSupported("processMessage")
		case .v2(_):
			throw ConversationError.v2NotSupported("processMessage")
		case let .group(group):
			return try await group.processMessage(envelopeBytes: envelopeBytes)
		case let .dm(dm):
			return try await dm.processMessage(envelopeBytes: envelopeBytes)
		}
	}
	
	public func prepareMessageV3<T>(content: T, options: SendOptions? = nil) async throws -> String {
		switch self {
		case .v1(_):
			throw ConversationError.v1NotSupported("prepareMessageV3 use prepareMessage instead")
		case .v2(_):
			throw ConversationError.v2NotSupported("prepareMessageV3 use prepareMessage instead")
		case let .group(group):
			return try await group.prepareMessage(content: content, options: options)
		case let .dm(dm):
			return try await dm.prepareMessage(content: content, options: options)
		}
	}

	public var version: Version {
		switch self {
		case .v1:
			return .v1
		case .v2:
			return .v2
		case .group:
			return .group
		case let .dm(dm):
			return .dm
		}
	}

	public var createdAt: Date {
		switch self {
		case let .v1(conversationV1):
			return conversationV1.sentAt
		case let .v2(conversationV2):
			return conversationV2.createdAt
		case let .group(group):
			return group.createdAt
		case let .dm(dm):
			return dm.createdAt
		}
	}

	@discardableResult public func send<T>(content: T, options: SendOptions? = nil, fallback _: String? = nil) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(content: content, options: options)
		case let .v2(conversationV2):
			return try await conversationV2.send(content: content, options: options)
		case let .group(group):
			return try await group.send(content: content, options: options)
		case let .dm(dm):
			return try await dm.send(content: content, options: options)
		}
	}

	@discardableResult public func send(encodedContent: EncodedContent, options: SendOptions? = nil) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(encodedContent: encodedContent, options: options)
		case let .v2(conversationV2):
			return try await conversationV2.send(encodedContent: encodedContent, options: options)
		case let .group(group):
			return try await group.send(content: encodedContent, options: options)
		case let .dm(dm):
			return try await dm.send(content: encodedContent, options: options)
		}
	}

	/// Send a message to the conversation
	public func send(text: String, options: SendOptions? = nil) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(content: text, options: options)
		case let .v2(conversationV2):
			return try await conversationV2.send(content: text, options: options)
		case let .group(group):
			return try await group.send(content: text, options: options)
		case let .dm(dm):
			return try await dm.send(content: text, options: options)
		}
	}

	public var clientAddress: String {
		return client.address
	}

	/// The topic identifier for this conversation
	public var topic: String {
		switch self {
		case let .v1(conversation):
			return conversation.topic.description
		case let .v2(conversation):
			return conversation.topic
		case let .group(group):
			return group.topic
		case let .dm(dm):
			return dm.topic
		}
	}
	
	/// Returns a stream you can iterate through to receive new messages in this conversation.
	///
	/// > Note: All messages in the conversation are returned by this stream. If you want to filter out messages
	/// by a sender, you can check the ``Client`` address against the message's ``peerAddress``.
	public func streamMessages() -> AsyncThrowingStream<DecodedMessage, Error> {
		switch self {
		case let .v1(conversation):
			return conversation.streamMessages()
		case let .v2(conversation):
			return conversation.streamMessages()
		case let .group(group):
			return group.streamMessages()
		case let .dm(dm):
			return dm.streamMessages()
		}
	}

	public func streamDecryptedMessages() -> AsyncThrowingStream<DecryptedMessage, Error> {
		switch self {
		case let .v1(conversation):
			return conversation.streamDecryptedMessages()
		case let .v2(conversation):
			return conversation.streamDecryptedMessages()
		case let .group(group):
			return group.streamDecryptedMessages()
		case let .dm(dm):
			return dm.streamDecryptedMessages()
		}
	}

	/// List messages in the conversation
	public func messages(limit: Int? = nil, before: Date? = nil, after: Date? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecodedMessage] {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.messages(limit: limit, before: before, after: after, direction: direction)
		case let .v2(conversationV2):
			return try await conversationV2.messages(limit: limit, before: before, after: after, direction: direction)
		case let .group(group):
			return try await group.messages(before: before, after: after, limit: limit, direction: direction)
		case let .dm(dm):
			return try await dm.messages(before: before, after: after, limit: limit, direction: direction)
		}
	}

	public func decryptedMessages(limit: Int? = nil, before: Date? = nil, after: Date? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecryptedMessage] {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.decryptedMessages(limit: limit, before: before, after: after, direction: direction)
		case let .v2(conversationV2):
			return try await conversationV2.decryptedMessages(limit: limit, before: before, after: after, direction: direction)
		case let .group(group):
			return try await group.decryptedMessages(before: before, after: after, limit: limit, direction: direction)
		case let .dm(dm):
			return try await dm.decryptedMessages(before: before, after: after, limit: limit, direction: direction)
		}
	}

	public var consentProof: ConsentProofPayload? {
		switch self {
		case .v1(_):
			return nil
		case let .v2(conversationV2):
			return conversationV2.consentProof
		case .group(_):
			return nil
		case let .dm(dm):
			return nil
		}
	}

	var client: Client {
		switch self {
		case let .v1(conversationV1):
			return conversationV1.client
		case let .v2(conversationV2):
			return conversationV2.client
		case let .group(group):
			return group.client
		case let .dm(dm):
			return dm.client
		}
	}
	
	// ------- V1 V2 to be deprecated ------
	
	public func encodedContainer() throws -> ConversationContainer  {
		switch self {
		case let .v1(conversationV1):
			return .v1(conversationV1.encodedContainer)
		case let .v2(conversationV2):
			return .v2(conversationV2.encodedContainer)
		case .group(_):
			throw ConversationError.v3NotSupported("encodedContainer")
		case .dm(_):
			throw ConversationError.v3NotSupported("encodedContainer")
		}
	}

	/// The wallet address of the other person in this conversation.
	public var peerAddress: String {
		get throws {
			switch self {
			case let .v1(conversationV1):
				return conversationV1.peerAddress
			case let .v2(conversationV2):
				return conversationV2.peerAddress
			case .group(_):
				throw ConversationError.v3NotSupported("peerAddress use members inboxId instead")
			case .dm(_):
				throw ConversationError.v3NotSupported("peerAddress use members inboxId instead")
			}
		}
	}

	public var peerAddresses: [String] {
		get throws {
			switch self {
			case let .v1(conversationV1):
				return [conversationV1.peerAddress]
			case let .v2(conversationV2):
				return [conversationV2.peerAddress]
			case .group(_):
				throw ConversationError.v3NotSupported("peerAddresses use members inboxIds instead")
			case .dm(_):
				throw ConversationError.v3NotSupported("peerAddresses use members inboxIds instead")
			}
		}
	}

	public var keyMaterial: Data? {
		switch self {
		case let .v1(conversationV1):
			return nil
		case let .v2(conversationV2):
			return conversationV2.keyMaterial
		case .group(_):
			return nil
		case .dm(_):
			return nil
		}
	}

	/// An optional string that can specify a different context for a conversation with another account address.
	///
	/// > Note: ``conversationID`` is only available for ``ConversationV2`` conversations.
	public var conversationID: String? {
		switch self {
		case .v1:
			return nil
		case let .v2(conversation):
			return conversation.context.conversationID
		case .group(_):
			return nil
		case .dm(_):
			return nil
		}
	}

	/// Exports the serializable topic data required for later import.
	/// See Conversations.importTopicData()
	public func toTopicData() throws -> Xmtp_KeystoreApi_V1_TopicMap.TopicData {
		try Xmtp_KeystoreApi_V1_TopicMap.TopicData.with {
			$0.createdNs = UInt64(createdAt.timeIntervalSince1970 * 1000) * 1_000_000
			$0.peerAddress = try peerAddress
			if case let .v2(cv2) = self {
				$0.invitation = Xmtp_MessageContents_InvitationV1.with {
					$0.topic = cv2.topic
					$0.context = cv2.context
					$0.aes256GcmHkdfSha256 = Xmtp_MessageContents_InvitationV1.Aes256gcmHkdfsha256.with {
						$0.keyMaterial = cv2.keyMaterial
					}
				}
			}
		}
	}

	public func decode(_ envelope: Envelope) throws -> DecodedMessage {
		switch self {
		case let .v1(conversationV1):
			return try conversationV1.decode(envelope: envelope)
		case let .v2(conversationV2):
			return try conversationV2.decode(envelope: envelope)
		case .group(_):
			throw ConversationError.v3NotSupported("decode use decodeV3 instead")
		case .dm(_):
			throw ConversationError.v3NotSupported("decode use decodeV3 instead")
		}
	}

	public func decrypt(_ envelope: Envelope) throws -> DecryptedMessage {
		switch self {
		case let .v1(conversationV1):
			return try conversationV1.decrypt(envelope: envelope)
		case let .v2(conversationV2):
			return try conversationV2.decrypt(envelope: envelope)
		case .group(_):
			throw ConversationError.v3NotSupported("decrypt use decryptV3 instead")
		case .dm(_):
			throw ConversationError.v3NotSupported("decrypt use decryptV3 instead")
		}
	}

	public func encode<Codec: ContentCodec, T>(codec: Codec, content: T) async throws -> Data where Codec.T == T {
		switch self {
		case let .v1:
			throw RemoteAttachmentError.v1NotSupported
		case let .v2(conversationV2):
			return try await conversationV2.encode(codec: codec, content: content)
		case .group(_):
			throw ConversationError.v3NotSupported("encode")
		case .dm(_):
			throw ConversationError.v3NotSupported("encode")
		}
	}

	public func prepareMessage(encodedContent: EncodedContent, options: SendOptions? = nil) async throws -> PreparedMessage {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.prepareMessage(encodedContent: encodedContent, options: options)
		case let .v2(conversationV2):
			return try await conversationV2.prepareMessage(encodedContent: encodedContent, options: options)
		case .group(_):
			throw ConversationError.v3NotSupported("prepareMessage use prepareMessageV3 instead")
		case .dm(_):
			throw ConversationError.v3NotSupported("prepareMessage use prepareMessageV3 instead")
		}
	}

	public func prepareMessage<T>(content: T, options: SendOptions? = nil) async throws -> PreparedMessage {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.prepareMessage(content: content, options: options ?? .init())
		case let .v2(conversationV2):
			return try await conversationV2.prepareMessage(content: content, options: options ?? .init())
		case .group(_):
			throw ConversationError.v3NotSupported("prepareMessage use prepareMessageV3 instead")
		case .dm(_):
			throw ConversationError.v3NotSupported("prepareMessage use prepareMessageV3 instead")
		}
	}

	// This is a convenience for invoking the underlying `client.publish(prepared.envelopes)`
	// If a caller has a `Client` handy, they may opt to do that directly instead.
	@discardableResult public func send(prepared: PreparedMessage) async throws -> String {
		switch self {
		case let .v1(conversationV1):
			return try await conversationV1.send(prepared: prepared)
		case let .v2(conversationV2):
			return try await conversationV2.send(prepared: prepared)
		case .group(_):
			throw ConversationError.v3NotSupported("send(prepareMessage) use send(content) instead")
		case .dm(_):
			throw ConversationError.v3NotSupported("send(prepareMessage) use send(content) instead")
		}
	}


	public func streamEphemeral() throws -> AsyncThrowingStream<Envelope, Error>? {
		switch self {
		case let .v1(conversation):
			return conversation.streamEphemeral()
		case let .v2(conversation):
			return conversation.streamEphemeral()
		case .group(_):
			throw ConversationError.v3NotSupported("streamEphemeral")
		case .dm(_):
			throw ConversationError.v3NotSupported("streamEphemeral")
		}
	}


}

extension Conversation: Hashable, Equatable {
	public static func == (lhs: Conversation, rhs: Conversation) -> Bool {
		lhs.topic == rhs.topic
	}

	public func hash(into hasher: inout Hasher) {
		hasher.combine(topic)
	}
}
