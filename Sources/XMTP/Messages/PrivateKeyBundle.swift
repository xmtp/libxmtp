//
//  PrivateKeyBundle.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import XMTPProto

public typealias PrivateKeyBundle = Xmtp_MessageContents_PrivateKeyBundle

enum PrivateKeyBundleError: Error {
	case noPreKeyFound
}

extension PrivateKeyBundle {
	init(v1: PrivateKeyBundleV1) {
		self.init()
		self.v1 = v1
	}

	func encrypted(with key: SigningKey) async throws -> EncryptedPrivateKeyBundle {
		let bundleBytes = try serializedData()
		let walletPreKey = try Crypto.secureRandomBytes(count: 32)

		let signature = try await key.sign(message: Signature.enableIdentityText(key: walletPreKey))
		let cipherText = try Crypto.encrypt(signature.rawDataWithNormalizedRecovery, bundleBytes)

		var encryptedBundle = EncryptedPrivateKeyBundle()
		encryptedBundle.v1.walletPreKey = walletPreKey
		encryptedBundle.v1.ciphertext = cipherText

		return encryptedBundle
	}
}
