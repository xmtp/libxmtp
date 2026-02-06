//
//  PreviewClientProvider.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import Foundation
import SwiftUI
import XMTPiOS

struct PreviewClientProvider<Content: View>: View {
	@State private var client: Client?
	@State private var error: String?
	var content: (Client) -> Content
	var wallet: PrivateKey

	init(@ViewBuilder _ content: @escaping (Client) -> Content) {
		self.content = content
		// swiftlint:disable force_try
		//
		// Generate a random private key. This won't give you much content.
		//
		wallet = try! PrivateKey.generate()
		//
		// You can provide your own private key if you have test data in
		// the local environment:
		//
		//		self.wallet = try! PrivateKey(Data(...))
		//
		// swiftlint:enable force_try
	}

	var body: some View {
		if let error {
			Text(error)
		}

		if let client {
			content(client)
		} else {
			Text("Creating client…")
				.task {
					do {
						let key = try secureRandomBytes(count: 32)
						Persistence().saveKeys(key)
						Persistence().saveAddress(wallet.identity.identifier)
						var options = ClientOptions(dbEncryptionKey: key)
						options.api.env = .dev
						options.api.isSecure = true
						let client = try await Client.create(account: wallet, options: options)
						await MainActor.run {
							self.client = client
						}
					} catch {
						self.error = "Error creating preview client: \(error)"
					}
				}
		}
	}
}

struct PreviewClientProvider_Previews: PreviewProvider {
	static var previews: some View {
		VStack {
			PreviewClientProvider { client in
				Text("Got our client: \(client.publicIdentity.identifier)")
			}
		}
	}
}

func secureRandomBytes(count: Int) throws -> Data {
	var bytes = [UInt8](repeating: 0, count: count)

	// Fill bytes with secure random data
	let status = SecRandomCopyBytes(
		kSecRandomDefault,
		count,
		&bytes,
	)

	// A status of errSecSuccess indicates success
	if status == errSecSuccess {
		return Data(bytes)
	} else {
		fatalError("could not generate random bytes")
	}
}
