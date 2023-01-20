//
//  WalletConnection.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import Foundation
import UIKit
import WalletConnectSwift
import web3
import XMTP

extension WCURL {
	var asURL: URL {
		// swiftlint:disable force_unwrapping
		URL(string: "wc://wc?uri=\(absoluteString)")!
		// swiftlint:enable force_unwrapping
	}
}

enum WalletConnectionError: String, Error {
	case walletConnectURL
	case noSession
	case noAddress
	case invalidMessage
	case noSignature
}

protocol WalletConnection {
	var isConnected: Bool { get }
	var walletAddress: String? { get }
	func preferredConnectionMethod() throws -> WalletConnectionMethodType
	func connect() async throws
	func sign(_ data: Data) async throws -> Data
}

class WCWalletConnection: WalletConnection, WalletConnectSwift.ClientDelegate {
	@Published public var isConnected = false

	var walletConnectClient: WalletConnectSwift.Client!
	var session: WalletConnectSwift.Session? {
		didSet {
			DispatchQueue.main.async {
				self.isConnected = self.session != nil
			}
		}
	}

	init() {
		let peerMeta = Session.ClientMeta(
			name: "xmtp-ios",
			description: "XMTP",
			icons: [],
			// swiftlint:disable force_unwrapping
			url: URL(string: "https://safe.gnosis.io")!
			// swiftlint:enable force_unwrapping
		)
		let dAppInfo = WalletConnectSwift.Session.DAppInfo(peerId: UUID().uuidString, peerMeta: peerMeta)

		walletConnectClient = WalletConnectSwift.Client(delegate: self, dAppInfo: dAppInfo)
	}

	func preferredConnectionMethod() throws -> WalletConnectionMethodType {
		guard let url = walletConnectURL?.asURL else {
			throw WalletConnectionError.walletConnectURL
		}

		if UIApplication.shared.canOpenURL(url) {
			return WalletRedirectConnectionMethod(redirectURI: url.absoluteString).type
		}

		return WalletQRCodeConnectionMethod(redirectURI: url.absoluteString).type
	}

	lazy var walletConnectURL: WCURL? = {
		do {
			let keybytes = try secureRandomBytes(count: 32)

			return WCURL(
				topic: UUID().uuidString,
				// swiftlint:disable force_unwrapping
				bridgeURL: URL(string: "https://bridge.walletconnect.org")!,
				// swiftlint:enable force_unwrapping
				key: keybytes.reduce("") { $0 + String(format: "%02x", $1) }
			)
		} catch {
			return nil
		}
	}()

	func secureRandomBytes(count: Int) throws -> Data {
		var bytes = [UInt8](repeating: 0, count: count)

		// Fill bytes with secure random data
		let status = SecRandomCopyBytes(
			kSecRandomDefault,
			count,
			&bytes
		)

		// A status of errSecSuccess indicates success
		if status == errSecSuccess {
			return Data(bytes)
		} else {
			fatalError("could not generate random bytes")
		}
	}

	func connect() async throws {
		guard let url = walletConnectURL else {
			throw WalletConnectionError.walletConnectURL
		}

		try walletConnectClient.connect(to: url)
	}

	func sign(_ data: Data) async throws -> Data {
		guard session != nil else {
			throw WalletConnectionError.noSession
		}

		guard let walletAddress = walletAddress else {
			throw WalletConnectionError.noAddress
		}

		guard let url = walletConnectURL else {
			throw WalletConnectionError.walletConnectURL
		}

		guard let message = String(data: data, encoding: .utf8) else {
			throw WalletConnectionError.invalidMessage
		}

		return try await withCheckedThrowingContinuation { continuation in
			do {
				try walletConnectClient.personal_sign(url: url, message: message, account: walletAddress) { response in
					if let error = response.error {
						continuation.resume(throwing: error)
						return
					}

					do {
						var resultString = try response.result(as: String.self)

						// Strip leading 0x that we get back from `personal_sign`
						if resultString.hasPrefix("0x"), resultString.count == 132 {
							resultString = String(resultString.dropFirst(2))
						}

						guard let resultDataBytes = resultString.web3.bytesFromHex else {
							continuation.resume(throwing: WalletConnectionError.noSignature)
							return
						}

						var resultData = Data(resultDataBytes)

						// Ensure we have a valid recovery byte
						resultData[resultData.count - 1] = 1 - resultData[resultData.count - 1] % 2

						continuation.resume(returning: resultData)
					} catch {
						continuation.resume(throwing: error)
					}
				}
			} catch {
				continuation.resume(throwing: error)
			}
		}
	}

	var walletAddress: String? {
		if let address = session?.walletInfo?.accounts.first {
			return EthereumAddress(address).toChecksumAddress()
		}

		return nil
	}

	func client(_: WalletConnectSwift.Client, didConnect _: WalletConnectSwift.WCURL) {}

	func client(_: WalletConnectSwift.Client, didFailToConnect _: WalletConnectSwift.WCURL) {}

	func client(_: WalletConnectSwift.Client, didConnect session: WalletConnectSwift.Session) {
		// TODO: Cache session
		self.session = session
	}

	func client(_: WalletConnectSwift.Client, didUpdate session: WalletConnectSwift.Session) {
		self.session = session
	}

	func client(_: WalletConnectSwift.Client, didDisconnect _: WalletConnectSwift.Session) {
		session = nil
	}
}
