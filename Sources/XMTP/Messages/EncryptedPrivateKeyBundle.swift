//
//  EncryptedPrivateKeyBundle.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import XMTPProto

typealias EncryptedPrivateKeyBundle = Xmtp_MessageContents_EncryptedPrivateKeyBundle

extension EncryptedPrivateKeyBundle {
	func decrypted(with key: SigningKey) async throws -> PrivateKeyBundle {
		let signature = try await key.sign(message: Signature.enableIdentityText(key: v1.walletPreKey))
		let message = try Crypto.decrypt(signature.rawDataWithNormalizedRecovery, v1.ciphertext)

		return try PrivateKeyBundle(serializedData: message)
	}
}
