//
//  Conversations.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

public enum ConversationError: Error {
	case recipientNotOnNetwork, recipientIsSender
}

/// Handles listing and creating Conversations.
public class Conversations {
	var client: Client
	var conversations: [Conversation] = []

	init(client: Client) {
		self.client = client
	}

	public func streamAllMessages() async throws -> AsyncThrowingStream<DecodedMessage, Error> {
		return AsyncThrowingStream { continuation in
			Task {
				while true {
					var topics: [String] = [
						Topic.userInvite(client.address).description,
						Topic.userIntro(client.address).description,
					]

					for conversation in try await list() {
						topics.append(conversation.topic)
					}

					do {
						for try await envelope in client.subscribe(topics: topics) {
							if let conversation = conversations.first(where: { $0.topic == envelope.contentTopic }) {
								let decoded = try conversation.decode(envelope)
								continuation.yield(decoded)
							} else if envelope.contentTopic.hasPrefix("/xmtp/0/invite-") {
								conversations.append(try fromInvite(envelope: envelope))
								break // Break so we can resubscribe with the new conversation
							} else if envelope.contentTopic.hasPrefix("/xmtp/0/intro-") {
								let conversation = try fromIntro(envelope: envelope)
								conversations.append(conversation)
								let decoded = try conversation.decode(envelope)
								continuation.yield(decoded)
								break // Break so we can resubscribe with the new conversation
							} else {
								print("huh \(envelope)")
							}
						}
					} catch {
						continuation.finish(throwing: error)
					}
				}
			}
		}
	}

	public func fromInvite(envelope: Envelope) throws -> Conversation {
		let sealedInvitation = try SealedInvitation(serializedData: envelope.message)
		let unsealed = try sealedInvitation.v1.getInvitation(viewer: client.keys)

		return .v2(try ConversationV2.create(client: client, invitation: unsealed, header: sealedInvitation.v1.header))
	}

	public func fromIntro(envelope: Envelope) throws -> Conversation {
		let messageV1 = try MessageV1.fromBytes(envelope.message)
		let senderAddress = try messageV1.header.sender.walletAddress
		let recipientAddress = try messageV1.header.recipient.walletAddress

		let peerAddress = client.address == senderAddress ? recipientAddress : senderAddress
		let conversationV1 = ConversationV1(client: client, peerAddress: peerAddress, sentAt: messageV1.sentAt)

		return .v1(conversationV1)
	}

	public func newConversation(with peerAddress: String, context: InvitationV1.Context? = nil) async throws -> Conversation {
		if peerAddress.lowercased() == client.address.lowercased() {
			throw ConversationError.recipientIsSender
		}

		if let existingConversation = conversations.first(where: { $0.peerAddress == peerAddress }) {
			return existingConversation
		}

		guard let contact = try await client.contacts.find(peerAddress) else {
			throw ConversationError.recipientNotOnNetwork
		}

		// See if we have an existing v1 convo
		if context?.conversationID == nil || context?.conversationID == "" {
			let invitationPeers = try await listIntroductionPeers()
			if let peerSeenAt = invitationPeers[peerAddress] {
				let conversation: Conversation = .v1(ConversationV1(client: client, peerAddress: peerAddress, sentAt: peerSeenAt))
				conversations.append(conversation)
				return conversation
			}
		}

		// If the contact is v1, start a v1 conversation
		if case .v1 = contact.version, context?.conversationID == nil || context?.conversationID == "" {
			let conversation: Conversation = .v1(ConversationV1(client: client, peerAddress: peerAddress, sentAt: Date()))
			conversations.append(conversation)
			return conversation
		}

		// See if we have a v2 conversation
		for sealedInvitation in try await listInvitations() {
			if !sealedInvitation.involves(contact) {
				continue
			}

			let invite = try sealedInvitation.v1.getInvitation(viewer: client.keys)
			if invite.context.conversationID == context?.conversationID, invite.context.conversationID != "" {
				let conversation: Conversation = .v2(ConversationV2(
					topic: invite.topic,
					keyMaterial: invite.aes256GcmHkdfSha256.keyMaterial,
					context: invite.context,
					peerAddress: peerAddress,
					client: client,
					header: sealedInvitation.v1.header
				))

				conversations.append(conversation)

				return conversation
			}
		}

		// We don't have an existing conversation, make a v2 one
		let recipient = try contact.toSignedPublicKeyBundle()
		let invitation = try InvitationV1.createRandom(context: context)

		let sealedInvitation = try await sendInvitation(recipient: recipient, invitation: invitation, created: Date())
		let conversationV2 = try ConversationV2.create(client: client, invitation: invitation, header: sealedInvitation.v1.header)

		let conversation: Conversation = .v2(conversationV2)
		conversations.append(conversation)
		return conversation
	}

	public func stream() -> AsyncThrowingStream<Conversation, Error> {
		AsyncThrowingStream { continuation in
			Task {
				var streamedConversationTopics: Set<String> = []

				for try await envelope in client.subscribe(topics: [.userIntro(client.address), .userInvite(client.address)]) {
					if envelope.contentTopic == Topic.userIntro(client.address).description {
						let conversationV1 = try fromIntro(envelope: envelope)

						if streamedConversationTopics.contains(conversationV1.topic.description) {
							continue
						}

						streamedConversationTopics.insert(conversationV1.topic.description)
						continuation.yield(conversationV1)
					}

					if envelope.contentTopic == Topic.userInvite(client.address).description {
						let conversationV2 = try fromInvite(envelope: envelope)

						if streamedConversationTopics.contains(conversationV2.topic) {
							continue
						}

						streamedConversationTopics.insert(conversationV2.topic)
						continuation.yield(conversationV2)
					}
				}
			}
		}
	}

	public func list() async throws -> [Conversation] {
		var conversations: [Conversation] = []

		do {
			let seenPeers = try await listIntroductionPeers()
			for (peerAddress, sentAt) in seenPeers {
				conversations.append(
					Conversation.v1(
						ConversationV1(
							client: client,
							peerAddress: peerAddress,
							sentAt: sentAt
						)
					)
				)
			}
		} catch {
			print("Error loading introduction peers: \(error)")
		}

		let invitations = try await listInvitations()

		for sealedInvitation in invitations {
			do {
				let unsealed = try sealedInvitation.v1.getInvitation(viewer: client.keys)
				let conversation = try ConversationV2.create(client: client, invitation: unsealed, header: sealedInvitation.v1.header)

				conversations.append(
					Conversation.v2(conversation)
				)
			} catch {
				print("Error loading invitations: \(error)")
			}
		}

		self.conversations = conversations.filter { $0.peerAddress != client.address }

		return self.conversations
	}

	func listIntroductionPeers() async throws -> [String: Date] {
		let envelopes = try await client.apiClient.query(
			topic: .userIntro(client.address),
			pagination: nil
		).envelopes

		let messages = envelopes.compactMap { envelope in
			do {
				let message = try MessageV1.fromBytes(envelope.message)

				// Attempt to decrypt, just to make sure we can
				_ = try message.decrypt(with: client.privateKeyBundleV1)

				return message
			} catch {
				return nil
			}
		}

		var seenPeers: [String: Date] = [:]
		for message in messages {
			guard let recipientAddress = message.recipientAddress,
			      let senderAddress = message.senderAddress
			else {
				continue
			}

			let sentAt = message.sentAt
			let peerAddress = recipientAddress == client.address ? senderAddress : recipientAddress

			guard let existing = seenPeers[peerAddress] else {
				seenPeers[peerAddress] = sentAt
				continue
			}

			if existing > sentAt {
				seenPeers[peerAddress] = sentAt
			}
		}

		return seenPeers
	}

	func listInvitations() async throws -> [SealedInvitation] {
		let envelopes = try await client.apiClient.envelopes(
			topic: Topic.userInvite(client.address).description,
			pagination: nil
		)

		return envelopes.compactMap { envelope in
			// swiftlint:disable no_optional_try
			try? SealedInvitation(serializedData: envelope.message)
			// swiftlint:enable no_optional_try
		}
	}

	func sendInvitation(recipient: SignedPublicKeyBundle, invitation: InvitationV1, created: Date) async throws -> SealedInvitation {
		let sealed = try SealedInvitation.createV1(
			sender: client.keys,
			recipient: recipient,
			created: created,
			invitation: invitation
		)

		let peerAddress = try recipient.walletAddress

		try await client.publish(envelopes: [
			Envelope(topic: .userInvite(client.address), timestamp: created, message: try sealed.serializedData()),
			Envelope(topic: .userInvite(peerAddress), timestamp: created, message: try sealed.serializedData()),
		])

		return sealed
	}
}
