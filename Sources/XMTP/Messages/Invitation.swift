//
//  Invitation.swift
//

import Foundation

/// Handles topic generation for conversations.
public typealias InvitationV1 = Xmtp_MessageContents_InvitationV1

extension InvitationV1 {
	static func createDeterministic(
			sender: PrivateKeyBundleV2,
			recipient: SignedPublicKeyBundle,
			context: InvitationV1.Context? = nil
	) throws -> InvitationV1 {
		let context = context ?? InvitationV1.Context()
        let myAddress = try sender.toV1().walletAddress
        let theirAddress = try recipient.walletAddress
        
		let secret = try sender.sharedSecret(
				peer: recipient,
				myPreKey: sender.preKeys[0].publicKey,
				isRecipient: myAddress < theirAddress)
		let addresses = [myAddress, theirAddress].sorted()
		let msg = "\(context.conversationID)\(addresses.joined(separator: ","))"
		let topicId = try Crypto.calculateMac(Data(msg.utf8), secret).toHex
		let topic = Topic.directMessageV2(topicId)

		let keyMaterial = try Crypto.deriveKey(
				secret: secret,
				nonce: Data("__XMTP__INVITATION__SALT__XMTP__".utf8),
				info: Data((["0"] + addresses).joined(separator: "|").utf8))

		var aes256GcmHkdfSha256 = InvitationV1.Aes256gcmHkdfsha256()
		aes256GcmHkdfSha256.keyMaterial = Data(keyMaterial)

		return try InvitationV1(
				topic: topic,
				context: context,
				aes256GcmHkdfSha256: aes256GcmHkdfSha256)
	}

	init(topic: Topic, context: InvitationV1.Context? = nil, aes256GcmHkdfSha256: InvitationV1.Aes256gcmHkdfsha256) throws {
		self.init()

		self.topic = topic.description

		if let context {
			self.context = context
		}

		self.aes256GcmHkdfSha256 = aes256GcmHkdfSha256
	}
}

/// Allows for additional data to be attached to V2 conversations
public extension InvitationV1.Context {
	init(conversationID: String = "", metadata: [String: String] = [:]) {
		self.init()
		self.conversationID = conversationID
		self.metadata = metadata
	}
}
