//
//  Crypto.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import CryptoKit
import Foundation
import XMTPProto

typealias CipherText = Xmtp_MessageContents_Ciphertext

enum CryptoError: Error {
	case randomBytes, combinedPayload
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

		ciphertext.aes256GcmHkdfSha256.payload = payload.ciphertext + payload.tag
		ciphertext.aes256GcmHkdfSha256.hkdfSalt = salt
		ciphertext.aes256GcmHkdfSha256.gcmNonce = nonceData

		return ciphertext
	}

	static func decrypt(_ secret: Data, _ ciphertext: CipherText, additionalData: Data? = nil) throws -> Data {
		let salt = ciphertext.aes256GcmHkdfSha256.hkdfSalt
		let nonceData = ciphertext.aes256GcmHkdfSha256.gcmNonce
		let nonce = try AES.GCM.Nonce(data: nonceData)
		let payload = ciphertext.aes256GcmHkdfSha256.payload.bytes

		let ciphertext = payload[0 ..< payload.count - 16]
		let tag = payload[payload.count - 16 ..< payload.count]
		let box = try AES.GCM.SealedBox(nonce: nonce, ciphertext: ciphertext, tag: tag)

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
}
