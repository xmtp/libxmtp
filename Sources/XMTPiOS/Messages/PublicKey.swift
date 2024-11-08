import CryptoKit
import Foundation
import LibXMTP
import web3

typealias PublicKey = Xmtp_MessageContents_PublicKey

enum PublicKeyError: String, Error {
	case noSignature, invalidPreKey, addressNotFound, invalidKeyString
}

extension PublicKey {
	var walletAddress: String {
		KeyUtilx.generateAddress(from: secp256K1Uncompressed.bytes)
			.toChecksumAddress()
	}
}
