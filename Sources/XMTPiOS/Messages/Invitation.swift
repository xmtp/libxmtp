//
//  Invitation.swift
//

import Foundation

/// Handles topic generation for conversations.
public typealias InvitationV1 = Xmtp_MessageContents_InvitationV1
public typealias ConsentProofPayload = Xmtp_MessageContents_ConsentProofPayload
public typealias ConsentProofPayloadVersion = Xmtp_MessageContents_ConsentProofPayloadVersion



extension InvitationV1 {
	static func createDeterministic(
			sender: PrivateKeyBundleV2,
			recipient: SignedPublicKeyBundle,
			context: InvitationV1.Context? = nil,
            consentProofPayload: ConsentProofPayload? = nil
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
				aes256GcmHkdfSha256: aes256GcmHkdfSha256,
                consentProof: consentProofPayload)
	}

    init(topic: Topic, context: InvitationV1.Context? = nil, aes256GcmHkdfSha256: InvitationV1.Aes256gcmHkdfsha256, consentProof: ConsentProofPayload? = nil) throws {
		self.init()

		self.topic = topic.description

		if let context {
			self.context = context
		}
        if let consentProof {
            self.consentProof = consentProof
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

extension ConsentProofPayload: Codable {
    enum CodingKeys: CodingKey {
        case signature, timestamp, payloadVersion
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(signature, forKey: .signature)
        try container.encode(timestamp, forKey: .timestamp)
        try container.encode(payloadVersion, forKey: .payloadVersion)
    }

    public init(from decoder: Decoder) throws {
        self.init()

        let container = try decoder.container(keyedBy: CodingKeys.self)
        signature = try container.decode(String.self, forKey: .signature)
        timestamp = try container.decode(UInt64.self, forKey: .timestamp)
        payloadVersion = try container.decode(Xmtp_MessageContents_ConsentProofPayloadVersion.self, forKey: .payloadVersion)
    }
}

extension ConsentProofPayloadVersion: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(Int.self)
        self = ConsentProofPayloadVersion(rawValue: rawValue) ?? ConsentProofPayloadVersion.UNRECOGNIZED(0)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(self.rawValue)
    }
}
