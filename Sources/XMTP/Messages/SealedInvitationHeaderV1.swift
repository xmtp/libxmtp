//
//  SealedInvitationHeaderV1.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import XMTPProto

public typealias SealedInvitationHeaderV1 = Xmtp_MessageContents_SealedInvitationHeaderV1

extension SealedInvitationHeaderV1 {
	init(sender: SignedPublicKeyBundle, recipient: SignedPublicKeyBundle, createdNs: UInt64) {
		self.init()
		self.sender = sender
		self.recipient = recipient
		self.createdNs = createdNs
	}
}

extension SealedInvitationHeaderV1: Codable {
	enum CodingKeys: CodingKey {
		case sender, recipient, createdNs
	}

	public func encode(to encoder: Encoder) throws {
		var container = encoder.container(keyedBy: CodingKeys.self)

		try container.encode(sender, forKey: .sender)
		try container.encode(recipient, forKey: .recipient)
		try container.encode(createdNs, forKey: .createdNs)
	}

	public init(from decoder: Decoder) throws {
		self.init()

		let container = try decoder.container(keyedBy: CodingKeys.self)
		sender = try container.decode(SignedPublicKeyBundle.self, forKey: .sender)
		recipient = try container.decode(SignedPublicKeyBundle.self, forKey: .recipient)
		createdNs = try container.decode(UInt64.self, forKey: .createdNs)
	}
}
