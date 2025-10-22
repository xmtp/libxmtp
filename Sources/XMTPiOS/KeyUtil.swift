import CryptoSwift
import Foundation

enum KeyUtilError: Error {
	case invalidContext
	case privateKeyInvalid
	case unknownError
	case signatureFailure
	case signatureParseFailure
	case badArguments
	case parseError
}

enum SignatureError: Error, CustomStringConvertible {
	case invalidMessage

	var description: String {
		"SignatureError.invalidMessage"
	}
}

enum KeyUtilx {
	static func generatePublicKey(from data: Data) throws -> Data {
		try ethereumGeneratePublicKey(privateKey32: data)
	}

	static func sign(message: Data, with privateKey: Data, hashing: Bool) throws -> Data {
		try ethereumSignRecoverable(msg: message, privateKey32: privateKey, hashing: hashing)
	}

	static func generateAddress(from publicKey: Data) throws -> String {
		try ethereumAddressFromPubkey(pubkey: publicKey)
	}

	static func ethHash(_ message: String) throws -> Data {
		try ethereumHashPersonal(message: message)
	}
}
