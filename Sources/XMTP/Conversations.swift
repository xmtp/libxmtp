//
//  Conversations.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

struct Conversations {
	var client: Client

	func list() async throws -> [Conversation] {
		var conversations: [Conversation] = []

		let seenPeers = try await listIntroductionPeers()
		let invitations = try await listInvitations()

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

		for sealedInvitation in invitations {
			let unsealed = try sealedInvitation.v1.getInvitation(viewer: client.keys)
			let conversation = try ConversationV2.create(client: client, invitation: unsealed, header: sealedInvitation.v1.header)

			conversations.append(
				Conversation.v2(conversation)
			)
		}

		return conversations
	}

	func listIntroductionPeers() async throws -> [String: Date] {
		let envelopes = try await client.apiClient.query(topics: [
			.userIntro(client.address),
		]).envelopes

		let messages = try envelopes.compactMap { envelope in
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
			      let senderAddress = message.senderAddress,
			      let sentAt = message.sentAt
			else {
				continue
			}

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
		let envelopes = try await client.apiClient.query(topics: [
			.userInvite(client.address),
		]).envelopes

		return try envelopes.map { envelope in
			try SealedInvitation(serializedData: envelope.message)
		}
	}

	func sendInvitation(recipient: SignedPublicKeyBundle, invitation: InvitationV1, created: Date) async throws -> SealedInvitation {
		let sealed = try SealedInvitation.createV1(
			sender: try client.privateKeyBundleV1.toV2(),
			recipient: recipient,
			created: created,
			invitation: invitation
		)

		let peerAddress = try recipient.walletAddress

		try await client.publish(envelopes: [
			Envelope(topic: .userInvite(peerAddress), timestamp: created, message: try sealed.serializedData()),
			Envelope(topic: .userInvite(client.address), timestamp: created, message: try sealed.serializedData()),
		])

		return sealed
	}
}
