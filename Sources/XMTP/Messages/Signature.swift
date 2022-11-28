//
//  Signature.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import XMTPProto

typealias Signature = Xmtp_MessageContents_Signature

enum SignatureError: Error {
	case invalidMessage
}

extension Signature {
	static func ethPersonalMessage(_ message: String) throws -> Data {
		let prefix = "\u{19}Ethereum Signed Message:\n\(message.count)"

		guard var data = prefix.data(using: .ascii) else {
			throw PrivateKeyError.invalidPrefix
		}

		guard let messageData = message.data(using: .utf8) else {
			throw SignatureError.invalidMessage
		}

		data.append(messageData)

		return data
	}

	static func ethHash(_ message: String) throws -> Data {
		let data = try ethPersonalMessage(message)

		return Util.keccak256(data)
	}

	static func createIdentityText(key: Data) -> String {
		return (
			"XMTP : Create Identity\n" +
				"\(key.toHex)\n" +
				"\n" +
				"For more info: https://xmtp.org/signatures/"
		)
	}

	static func enableIdentityText(key: Data) -> String {
		return (
			"XMTP : Enable Identity\n" +
				"\(key.toHex)\n" +
				"\n" +
				"For more info: https://xmtp.org/signatures/"
		)
	}

	var rawData: Data {
		switch union {
		case .ecdsaCompact(ecdsaCompact):
			return ecdsaCompact.bytes + [UInt8(Int(ecdsaCompact.recovery))]
		case .walletEcdsaCompact(walletEcdsaCompact):
			return walletEcdsaCompact.bytes + [UInt8(Int(walletEcdsaCompact.recovery))]
		case .none:
			return Data()
		case .some:
			return Data()
		}
	}
}
