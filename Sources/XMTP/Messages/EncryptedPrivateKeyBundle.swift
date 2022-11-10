//
//  EncryptedPrivateKeyBundle.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import XMTPProto

typealias EncryptedPrivateKeyBundle = Xmtp_MessageContents_EncryptedPrivateKeyBundle

extension EncryptedPrivateKeyBundle {
	func decrypted(with _: SigningKey) async throws -> PrivateKeyBundle {
		let signature = v1.walletPreKey
		let message = try Crypto.decrypt(signature, v1.ciphertext)

		return try PrivateKeyBundle(serializedData: message)
	}
}
