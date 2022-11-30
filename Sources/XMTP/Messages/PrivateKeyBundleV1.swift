//
//  PrivateKeyBundleV1.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import XMTPProto

typealias PrivateKeyBundleV1 = Xmtp_MessageContents_PrivateKeyBundleV1

extension PrivateKeyBundleV1 {
	static func generate(wallet: SigningKey) async throws -> PrivateKeyBundleV1 {
		let privateKey = try PrivateKey.generate()
		let authorizedIdentity = try await wallet.createIdentity(privateKey)

		var bundle = try authorizedIdentity.toBundle
		var preKey = try PrivateKey.generate()

		let signedPublicKey = try await privateKey.sign(key: UnsignedPublicKey(preKey.publicKey))

		preKey.publicKey = try PublicKey(serializedData: signedPublicKey.keyBytes)
		preKey.publicKey.signature = signedPublicKey.signature
		bundle.v1.preKeys = [preKey]

		return bundle.v1
	}

	func toPublicKeyBundle() -> PublicKeyBundle {
		var publicKeyBundle = PublicKeyBundle()

		publicKeyBundle.identityKey = identityKey.publicKey
		publicKeyBundle.preKey = preKeys[0].publicKey

		return publicKeyBundle
	}
}
