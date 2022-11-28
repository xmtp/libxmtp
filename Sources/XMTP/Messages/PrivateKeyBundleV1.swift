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
		let preKey = try PrivateKey.generate()
		bundle.v1.preKeys = [preKey]

		return bundle.v1
	}
}
