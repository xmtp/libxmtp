//
//  PrivateKeyBundleV1.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import CryptoKit
import Foundation
import XMTPProto

public typealias PrivateKeyBundleV1 = Xmtp_MessageContents_PrivateKeyBundleV1

extension PrivateKeyBundleV1 {
	static func generate(wallet: SigningKey) async throws -> PrivateKeyBundleV1 {
		let privateKey = try PrivateKey.generate()
		let authorizedIdentity = try await wallet.createIdentity(privateKey)

		var bundle = try authorizedIdentity.toBundle
		var preKey = try PrivateKey.generate()

		let bytesToSign = try UnsignedPublicKey(preKey.publicKey).serializedData()
		let signature = try await privateKey.sign(Data(SHA256.hash(data: bytesToSign)))

		bundle.v1.identityKey = authorizedIdentity.identity
		bundle.v1.identityKey.publicKey = authorizedIdentity.authorized
		preKey.publicKey.signature = signature

		let signedPublicKey = try await privateKey.sign(key: UnsignedPublicKey(preKey.publicKey))

		preKey.publicKey = try PublicKey(serializedData: signedPublicKey.keyBytes)
		preKey.publicKey.signature = signedPublicKey.signature
		bundle.v1.preKeys = [preKey]

		return bundle.v1
	}

	var walletAddress: String {
		// swiftlint:disable no_optional_try
		return (try? identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress) ?? ""
		// swiftlint:enable no_optional_try
	}

	func toV2() -> PrivateKeyBundleV2 {
		var v2bundle = PrivateKeyBundleV2()

		v2bundle.identityKey = SignedPrivateKey.fromLegacy(identityKey, signedByWallet: false)
		v2bundle.preKeys = preKeys.map { SignedPrivateKey.fromLegacy($0) }

		return v2bundle
	}

	func toPublicKeyBundle() -> PublicKeyBundle {
		var publicKeyBundle = PublicKeyBundle()

		publicKeyBundle.identityKey = identityKey.publicKey
		publicKeyBundle.preKey = preKeys[0].publicKey

		return publicKeyBundle
	}

	func sharedSecret(peer: PublicKeyBundle, myPreKey: PublicKey, isRecipient: Bool) throws -> Data {
		let peerBundle = try SignedPublicKeyBundle(peer)
		let preKey = SignedPublicKey.fromLegacy(myPreKey)

		return try toV2().sharedSecret(peer: peerBundle, myPreKey: preKey, isRecipient: isRecipient)
	}
}
