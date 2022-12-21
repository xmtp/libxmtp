//
//  Invitation.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

/// Handles topic generation for conversations.
public typealias InvitationV1 = Xmtp_MessageContents_InvitationV1

extension InvitationV1 {
	static func createRandom(context: InvitationV1.Context? = nil) throws -> InvitationV1 {
		let context = context ?? InvitationV1.Context()
		let randomBytes = try Crypto.secureRandomBytes(count: 32)
		let randomString = Data(randomBytes).base64EncodedString()
			.replacingOccurrences(of: "=*$", with: "", options: .regularExpression)
			.replacingOccurrences(of: "[^A-Za-z0-9]", with: "", options: .regularExpression)

		let topic = Topic.directMessageV2(randomString)

		let keyMaterial = try Crypto.secureRandomBytes(count: 32)

		var aes256GcmHkdfSha256 = InvitationV1.Aes256gcmHkdfsha256()
		aes256GcmHkdfSha256.keyMaterial = Data(keyMaterial)

		return try InvitationV1(
			topic: topic,
			context: context,
			aes256GcmHkdfSha256: aes256GcmHkdfSha256
		)
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
