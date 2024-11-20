import Foundation
import CSecp256k1
import LibXMTP
import CryptoSwift

enum KeyUtilError: Error {
	case invalidContext
	case privateKeyInvalid
	case unknownError
	case signatureFailure
	case signatureParseFailure
	case badArguments
	case parseError
}

enum KeyUtilx {
	static func generatePublicKey(from data: Data) throws -> Data {
		let vec = try LibXMTP.publicKeyFromPrivateKeyK256(privateKeyBytes: data)
		return Data(vec)
	}

	static func recoverPublicKeySHA256(from data: Data, message: Data) throws -> Data {
		return try Data(LibXMTP.recoverPublicKeyK256Sha256(message: message, signature: data))
	}

	static func recoverPublicKeyKeccak256(from data: Data, message: Data) throws -> Data {
		return Data(try LibXMTP.recoverPublicKeyK256Keccak256(message: message, signature: data))
	}
	
	static func sign(message: Data, with privateKey: Data, hashing: Bool) throws -> Data {
		guard let ctx = secp256k1_context_create(UInt32(SECP256K1_CONTEXT_SIGN | SECP256K1_CONTEXT_VERIFY)) else {
			throw KeyUtilError.invalidContext
		}

		defer {
			secp256k1_context_destroy(ctx)
		}

		let msgData = hashing ? Util.keccak256(message) : message
		let msg = (msgData as NSData).bytes.assumingMemoryBound(to: UInt8.self)
		let privateKeyPtr = (privateKey as NSData).bytes.assumingMemoryBound(to: UInt8.self)
		let signaturePtr = UnsafeMutablePointer<secp256k1_ecdsa_recoverable_signature>.allocate(capacity: 1)
		defer {
			signaturePtr.deallocate()
		}
		guard secp256k1_ecdsa_sign_recoverable(ctx, signaturePtr, msg, privateKeyPtr, nil, nil) == 1 else {
			throw KeyUtilError.signatureFailure
		}

		let outputPtr = UnsafeMutablePointer<UInt8>.allocate(capacity: 64)
		defer {
			outputPtr.deallocate()
		}
		var recid: Int32 = 0
		secp256k1_ecdsa_recoverable_signature_serialize_compact(ctx, outputPtr, &recid, signaturePtr)

		let outputWithRecidPtr = UnsafeMutablePointer<UInt8>.allocate(capacity: 65)
		defer {
			outputWithRecidPtr.deallocate()
		}
		outputWithRecidPtr.assign(from: outputPtr, count: 64)
		outputWithRecidPtr.advanced(by: 64).pointee = UInt8(recid)

		let signature = Data(bytes: outputWithRecidPtr, count: 65)

		return signature
	}

	static func generateAddress(from publicKey: Data) -> String {
		let publicKeyData = publicKey.count == 64 ? publicKey : publicKey[1 ..< publicKey.count]

		let hash = publicKeyData.sha3(.keccak256)
		let address = hash.subdata(in: 12 ..< hash.count)
		return "0x" + address.toHex
	}
}
