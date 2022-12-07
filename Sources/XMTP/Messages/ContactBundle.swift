//
//  ContactBundle.swift
//
//
//  Created by Pat Nakajima on 11/23/22.
//

import XMTPProto

typealias ContactBundle = Xmtp_MessageContents_ContactBundle
typealias ContactBundleV1 = Xmtp_MessageContents_ContactBundleV1
typealias ContactBundleV2 = Xmtp_MessageContents_ContactBundleV2

enum ContactBundleError: Error {
	case invalidVersion, notFound
}

extension ContactBundle {
	static func from(envelope: Envelope) throws -> ContactBundle {
		let data = envelope.message

		var contactBundle = ContactBundle()

		// Try to deserialize legacy v1 bundle
		let publicKeyBundle = try PublicKeyBundle(serializedData: data)

		contactBundle.v1.keyBundle = publicKeyBundle

		// It's not a legacy bundle so just deserialize as a ContactBundle
		if contactBundle.v1.keyBundle.identityKey.secp256K1Uncompressed.bytes.isEmpty {
			try contactBundle.merge(serializedData: data)
		}

		return contactBundle
	}

	func toPublicKeyBundle() throws -> PublicKeyBundle {
		switch version {
		case .v1:
			return v1.keyBundle
		case .v2:
			return try PublicKeyBundle(v2.keyBundle)
		default:
			throw ContactBundleError.invalidVersion
		}
	}

	func toSignedPublicKeyBundle() throws -> SignedPublicKeyBundle {
		switch version {
		case .v1:
			return try SignedPublicKeyBundle(v1.keyBundle)
		case .v2:
			return v2.keyBundle
		case .none:
			throw ContactBundleError.invalidVersion
		}
	}

	// swiftlint:disable no_optional_try

	var walletAddress: String? {
		switch version {
		case .v1:
			if let key = try? v1.keyBundle.identityKey.recoverWalletSignerPublicKey() {
				return KeyUtil.generateAddress(from: key.secp256K1Uncompressed.bytes).toChecksumAddress()
			}

			return nil
		case .v2:
			if let key = try? v2.keyBundle.identityKey.recoverWalletSignerPublicKey() {
				return KeyUtil.generateAddress(from: key.secp256K1Uncompressed.bytes).toChecksumAddress()
			}

			return nil
		case .none:
			return nil
		}
	}

	var identityAddress: String? {
		switch version {
		case .v1:
			return v1.keyBundle.identityKey.walletAddress
		case .v2:
			let publicKey = try? PublicKey(v2.keyBundle.identityKey)
			return publicKey?.walletAddress
		case .none:
			return nil
		}
	}

	// swiftlint:enable no_optional_try
}
