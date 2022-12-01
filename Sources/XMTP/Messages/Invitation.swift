//
//  Invitation.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

typealias InvitationV1 = Xmtp_MessageContents_InvitationV1

extension InvitationV1 {
	@available(iOS 16.0, *)
	static func createRandom(context: InvitationV1.Context? = nil) throws -> InvitationV1 {
		var context = context ?? InvitationV1.Context()
		let randomBytes = try Crypto.secureRandomBytes(count: 32)
		let regex = #/=*$/#
		let randomString = Data(randomBytes).base64EncodedString()
			.replacing(regex, with: "")
			.replacingOccurrences(of: "/", with: "-")

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
