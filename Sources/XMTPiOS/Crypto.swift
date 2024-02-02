//
//  Crypto.swift
//

import CryptoKit
import Foundation

public typealias CipherText = Xmtp_MessageContents_Ciphertext

enum CryptoError: Error {
	case randomBytes, combinedPayload, keyDerivationError, hmacSignatureError
}

enum Crypto {
	static func encrypt(_ secret: Data, _ message: Data, additionalData: Data? = nil) throws -> CipherText {
		let salt = try secureRandomBytes(count: 32)
		let nonceData = try secureRandomBytes(count: 12)
		let nonce = try AES.GCM.Nonce(data: nonceData)

		let resultKey = HKDF<SHA256>.deriveKey(
			inputKeyMaterial: SymmetricKey(data: secret),
			salt: salt,
			outputByteCount: 32
		)

		var payload: AES.GCM.SealedBox

		if let additionalData {
			payload = try AES.GCM.seal(message, using: resultKey, nonce: nonce, authenticating: additionalData)
		} else {
			payload = try AES.GCM.seal(message, using: resultKey, nonce: nonce)
		}

		var ciphertext = CipherText()
		// Copy the ciphertext data out, otherwise it's a region sliced from a combined Data (nonce, ciphertext, tag)
		// with offsets like lowerBound=12, upperBound=224. Without copying, trying to index like payload[0] crashes
		// up until payload[12]. This is mostly a problem for unit tests where we decrypt what we encrypt in memory, as
		// serialization/deserialization acts as copying and avoids this issue.
		var payloadData = Data(payload.ciphertext.subdata(in: 12 ..< payload.ciphertext.count+12))
		let startTag = 12 + payload.ciphertext.count
		payloadData.append(payload.tag.subdata(in: startTag ..< startTag + payload.tag.count))
		ciphertext.aes256GcmHkdfSha256.payload = payloadData
		ciphertext.aes256GcmHkdfSha256.hkdfSalt = salt
		ciphertext.aes256GcmHkdfSha256.gcmNonce = nonceData

		return ciphertext
	}

	static func decrypt(_ secret: Data, _ ciphertext: CipherText, additionalData: Data? = nil) throws -> Data {
		let salt = ciphertext.aes256GcmHkdfSha256.hkdfSalt
		let nonceData = ciphertext.aes256GcmHkdfSha256.gcmNonce
		let nonce = try AES.GCM.Nonce(data: nonceData)
		let payload = ciphertext.aes256GcmHkdfSha256.payload

		let ciphertextBytes = payload[0 ..< payload.count - 16]
		let tag = payload[payload.count - 16 ..< payload.count]
		let box = try AES.GCM.SealedBox(nonce: nonce, ciphertext: ciphertextBytes, tag: tag)

		let resultKey = HKDF<SHA256>.deriveKey(
			inputKeyMaterial: SymmetricKey(data: secret),
			salt: salt,
			outputByteCount: 32
		)

		if let additionalData {
			return try AES.GCM.open(box, using: resultKey, authenticating: additionalData)
		} else {
			return try AES.GCM.open(box, using: resultKey)
		}
	}

	static func calculateMac(_ message: Data, _ secret: Data) throws -> Data {
		let mac = HMAC<SHA256>.authenticationCode(for: message, using: SymmetricKey(data: secret))
		return Data(mac)
	}

	static func deriveKey(secret: Data, nonce: Data, info: Data) throws -> Data {
		let key = HKDF<SHA256>.deriveKey(
				inputKeyMaterial: SymmetricKey(data: secret),
				salt: nonce,
				info: info,
				outputByteCount: 32
		)
        return key.withUnsafeBytes { body in
            Data(body)
        }
	}

	static func secureRandomBytes(count: Int) throws -> Data {
		var bytes = [UInt8](repeating: 0, count: count)

		// Fill bytes with secure random data
		let status = SecRandomCopyBytes(
			kSecRandomDefault,
			count,
			&bytes
		)

		// A status of errSecSuccess indicates success
		if status == errSecSuccess {
			return Data(bytes)
		} else {
			throw CryptoError.randomBytes
		}
	}
	
	static func hkdfHmacKey(secret: Data, info: Data) throws -> SymmetricKey {
		do {
			let salt = try secureRandomBytes(count: 32)
			let key = HKDF<SHA256>.deriveKey(
				inputKeyMaterial: SymmetricKey(data: secret),
				salt: salt,
				info: info,
				outputByteCount: 32)
			return key
		} catch {
			throw CryptoError.keyDerivationError
		}
	}
	
	static func generateHmacSignature(secret: Data, info: Data, message: Data) throws -> Data {
		do {
		  let key = try hkdfHmacKey(secret: secret, info: info)
		  let signature = HMAC<SHA256>.authenticationCode(for: message, using: key)
		  return Data(signature)
	  } catch {
		  throw CryptoError.hmacSignatureError
	  }
	}
	
	static func exportHmacKey(key: SymmetricKey) -> Data {
		var exportedData = Data(count: key.bitCount / 8)
		exportedData.withUnsafeMutableBytes { buffer in
			key.withUnsafeBytes { keyBuffer in
				buffer.copyMemory(from: keyBuffer)
			}
		}
		return exportedData
	}

	static func importHmacKey(keyData: Data) -> SymmetricKey {
		return SymmetricKey(data: keyData)
	}
}
