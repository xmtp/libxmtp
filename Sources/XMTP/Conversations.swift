//
//  Conversations.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

public struct Conversations {
	var client: Client

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

		return conversations
	}

	func listIntroductionPeers() async throws -> [String: Date] {
		let envelopes = try await client.apiClient.query(topics: [
			.userIntro(client.address),
		]).envelopes

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
		let envelopes = try await client.apiClient.query(topics: [
			.userInvite(client.address),
		]).envelopes

		return envelopes.compactMap { envelope in
			// swiftlint:disable no_optional_try
			try? SealedInvitation(serializedData: envelope.message)
			// swiftlint:enable no_optional_try
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
		])

		return sealed
	}
}
