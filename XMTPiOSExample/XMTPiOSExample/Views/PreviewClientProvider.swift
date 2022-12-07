//
//  PreviewClientProvider.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import Foundation
import SwiftUI
import XMTP

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
						var options = ClientOptions()
						options.api.env = .local
						options.api.isSecure = false
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
				Text("Got our client: \(client.address)")
			}
		}
	}
}
