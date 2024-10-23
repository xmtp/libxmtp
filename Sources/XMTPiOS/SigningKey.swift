//
//  SigningKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import web3
import LibXMTP

public enum WalletType {
	case EOA, SCW
}

/// Defines a type that is used by a ``Client`` to sign keys and messages.
///
/// You can use ``Account`` for an easier WalletConnect flow, or ``PrivateKey``
/// for quick key generation.
///
/// > Tip: You can make your own object that conforms to ``SigningKey`` if you want to
/// handle key management yourself.
public protocol SigningKey {
	/// A wallet address for this key
	var address: String { get }
	
	/// The wallet type if Smart Contract Wallet this should be type SCW. Default EOA
	var type: WalletType { get }
	
	/// The name of the chainId for example "1"
	var chainId: Int64? { get }
	
	/// The blockNumber of the chain for example "1"
	var blockNumber: Int64? { get }

	/// Sign the data and return a secp256k1 compact recoverable signature.
	func sign(_ data: Data) async throws -> Signature

	/// Pass a personal Ethereum signed message string text to be signed, returning
	/// a secp256k1 compact recoverable signature. You can use ``Signature.ethPersonalMessage`` to generate this text.
	func sign(message: String) async throws -> Signature
	
	/// Pass a personal Ethereum signed message string text to be signed, return bytes to be verified
	func signSCW(message: String) async throws -> Data
}

extension SigningKey {
	public var type: WalletType {
		return WalletType.EOA
	}
	
	public var chainId: Int64? {
		return nil
	}
	
	public var blockNumber: Int64? {
		return nil
	}

	func createIdentity(_ identity: PrivateKey, preCreateIdentityCallback: PreEventCallback? = nil) async throws -> AuthorizedIdentity {
		var slimKey = PublicKey()
		slimKey.timestamp = UInt64(Date().millisecondsSinceEpoch)
		slimKey.secp256K1Uncompressed = identity.publicKey.secp256K1Uncompressed

		try await preCreateIdentityCallback?()

		let signatureText = Signature.createIdentityText(key: try slimKey.serializedData())
		let signature = try await sign(message: signatureText)

		let message = try Signature.ethPersonalMessage(signatureText)
		let recoveredKey = try KeyUtilx.recoverPublicKeyKeccak256(from: signature.rawData, message: message)
		let address = KeyUtilx.generateAddress(from: recoveredKey).toChecksumAddress()

		var authorized = PublicKey()
		authorized.secp256K1Uncompressed = slimKey.secp256K1Uncompressed
		authorized.timestamp = slimKey.timestamp
		authorized.signature = signature

		return AuthorizedIdentity(address: address, authorized: authorized, identity: identity)
	}
	
	public func sign(_ data: Data) async throws -> Signature {
		throw NSError(domain: "NotImplemented", code: 1, userInfo: [NSLocalizedDescriptionKey: "sign(Data) not implemented."])
	}

	public func sign(message: String) async throws -> Signature {
		throw NSError(domain: "NotImplemented", code: 1, userInfo: [NSLocalizedDescriptionKey: "sign(String) not implemented."])
	}

	public func signSCW(message: String) async throws -> Data {
		throw NSError(domain: "NotImplemented", code: 1, userInfo: [NSLocalizedDescriptionKey: "signSCW(String) not implemented."])
	}
}
