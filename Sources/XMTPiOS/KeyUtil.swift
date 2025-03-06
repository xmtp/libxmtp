import CSecp256k1
import CryptoSwift
import Foundation
import LibXMTP

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
		guard data.count == 32 else { throw KeyUtilError.privateKeyInvalid }

		// Initialize secp256k1 context
		guard
			let ctx = secp256k1_context_create(
				UInt32(SECP256K1_CONTEXT_SIGN | SECP256K1_CONTEXT_VERIFY))
		else {
			throw KeyUtilError.invalidContext
		}
		defer { secp256k1_context_destroy(ctx) }  // Ensure context is freed

		var pubKey = secp256k1_pubkey()
		let status = data.withUnsafeBytes { privKeyPointer -> Int32 in
			guard
				let privKeyBaseAddress = privKeyPointer.baseAddress?
					.assumingMemoryBound(to: UInt8.self)
			else {
				return 0  // Indicate failure if memory binding fails
			}
			return secp256k1_ec_pubkey_create(ctx, &pubKey, privKeyBaseAddress)
		}

		guard status == 1 else { throw KeyUtilError.privateKeyInvalid }  // Failed to create public key

		// Serialize public key (uncompressed 65 bytes or compressed 33 bytes)
		var output = Data(repeating: 0, count: 65)
		var outputLength = output.count

		let result = output.withUnsafeMutableBytes { outputPointer -> Int32 in
			guard
				let outputBaseAddress = outputPointer.baseAddress?
					.assumingMemoryBound(to: UInt8.self)
			else {
				return 0  // Indicate failure if memory binding fails
			}
			return secp256k1_ec_pubkey_serialize(
				ctx,
				outputBaseAddress,
				&outputLength,
				&pubKey,
				UInt32(SECP256K1_EC_UNCOMPRESSED)  // Use `SECP256K1_EC_COMPRESSED` for 33 bytes
			)
		}

		guard result == 1 else {
			throw KeyUtilError.parseError
		}  // Ensure serialization succeeded

		return output.prefix(outputLength)
	}

	static func sign(message: Data, with privateKey: Data, hashing: Bool) throws
		-> Data
	{
		guard
			let ctx = secp256k1_context_create(
				UInt32(SECP256K1_CONTEXT_SIGN | SECP256K1_CONTEXT_VERIFY))
		else {
			throw KeyUtilError.invalidContext
		}

		defer {
			secp256k1_context_destroy(ctx)
		}

		let msgData = hashing ? Util.keccak256(message) : message
		let msg = (msgData as NSData).bytes.assumingMemoryBound(to: UInt8.self)
		let privateKeyPtr = (privateKey as NSData).bytes.assumingMemoryBound(
			to: UInt8.self)
		let signaturePtr = UnsafeMutablePointer<
			secp256k1_ecdsa_recoverable_signature
		>.allocate(capacity: 1)
		defer {
			signaturePtr.deallocate()
		}
		guard
			secp256k1_ecdsa_sign_recoverable(
				ctx, signaturePtr, msg, privateKeyPtr, nil, nil) == 1
		else {
			throw KeyUtilError.signatureFailure
		}

		let outputPtr = UnsafeMutablePointer<UInt8>.allocate(capacity: 64)
		defer {
			outputPtr.deallocate()
		}
		var recid: Int32 = 0
		secp256k1_ecdsa_recoverable_signature_serialize_compact(
			ctx, outputPtr, &recid, signaturePtr)

		let outputWithRecidPtr = UnsafeMutablePointer<UInt8>.allocate(
			capacity: 65)
		defer {
			outputWithRecidPtr.deallocate()
		}
		outputWithRecidPtr.assign(from: outputPtr, count: 64)
		outputWithRecidPtr.advanced(by: 64).pointee = UInt8(recid)

		let signature = Data(bytes: outputWithRecidPtr, count: 65)

		return signature
	}

	static func generateAddress(from publicKey: Data) -> String {
		let publicKeyData =
			publicKey.count == 64 ? publicKey : publicKey[1..<publicKey.count]

		let hash = publicKeyData.sha3(.keccak256)
		let address = hash.subdata(in: 12..<hash.count)
		return "0x" + address.toHex
	}
}
