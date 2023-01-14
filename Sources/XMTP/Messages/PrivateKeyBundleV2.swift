//
//  PrivateKeyBundleV2.swift
//
//
//  Created by Pat Nakajima on 11/26/22.
//

import Foundation
import secp256k1
import XMTPProto

public typealias PrivateKeyBundleV2 = Xmtp_MessageContents_PrivateKeyBundleV2

extension PrivateKeyBundleV2 {
	func sharedSecret(peer: SignedPublicKeyBundle, myPreKey: SignedPublicKey, isRecipient: Bool) throws -> Data {
		var dh1: Data
		var dh2: Data
		var preKey: SignedPrivateKey

		if isRecipient {
			preKey = try findPreKey(myPreKey)
			dh1 = try sharedSecret(private: preKey.secp256K1.bytes, public: peer.identityKey.secp256K1Uncompressed.bytes)
			dh2 = try sharedSecret(private: identityKey.secp256K1.bytes, public: peer.preKey.secp256K1Uncompressed.bytes)
		} else {
			preKey = try findPreKey(myPreKey)
			dh1 = try sharedSecret(private: identityKey.secp256K1.bytes, public: peer.preKey.secp256K1Uncompressed.bytes)
			dh2 = try sharedSecret(private: preKey.secp256K1.bytes, public: peer.identityKey.secp256K1Uncompressed.bytes)
		}

		let dh3 = try sharedSecret(private: preKey.secp256K1.bytes, public: peer.preKey.secp256K1Uncompressed.bytes)

		let secret = dh1 + dh2 + dh3

		return secret
	}

	func sharedSecret(private privateData: Data, public publicData: Data) throws -> Data {
		let publicKey = try secp256k1.Signing.PublicKey(rawRepresentation: publicData, format: .uncompressed)

		let sharedSecret = try publicKey.multiply(privateData.bytes, format: .uncompressed)

		return sharedSecret.rawRepresentation
	}

	func findPreKey(_ myPreKey: SignedPublicKey) throws -> SignedPrivateKey {
		for preKey in preKeys {
			if preKey.matches(myPreKey) {
				return preKey
			}
		}

		throw PrivateKeyBundleError.noPreKeyFound
	}

	func toV1() throws -> PrivateKeyBundleV1 {
		var bundle = PrivateKeyBundleV1()
		bundle.identityKey = try PrivateKey(identityKey)
		bundle.preKeys = try preKeys.map { try PrivateKey($0) }
		return bundle
	}

	func getPublicKeyBundle() -> SignedPublicKeyBundle {
		var publicKeyBundle = SignedPublicKeyBundle()

		publicKeyBundle.identityKey = identityKey.publicKey
		publicKeyBundle.identityKey.signature = identityKey.publicKey.signature
		publicKeyBundle.identityKey.signature.ensureWalletSignature()
		publicKeyBundle.preKey = preKeys[0].publicKey

		return publicKeyBundle
	}
}
