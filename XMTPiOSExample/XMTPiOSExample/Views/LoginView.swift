//
//  LoginView.swift
//  XMTPChat
//
//  Created by Pat Nakajima on 6/7/23.
//

import SwiftUI
import Combine
import WebKit
import XMTPiOS
import WalletConnectRelay
import WalletConnectModal
import Starscream

extension WebSocket: WebSocketConnecting { }
extension Blockchain: @unchecked Sendable { }

struct SocketFactory: WebSocketFactory {
	func create(with url: URL) -> WalletConnectRelay.WebSocketConnecting {
		WebSocket(url: url)
	}
}

// WalletConnectV2's ModalSheet doesn't have any public initializers so we need
// to wrap their UIKit API
struct ModalWrapper: UIViewControllerRepresentable {
	func makeUIViewController(context: Context) -> UIViewController {
		let controller = UIViewController()
		Task {
			try? await Task.sleep(for: .seconds(0.4))
			await MainActor.run {
				WalletConnectModal.present(from: controller)
			}
		}
		return controller
	}

	func updateUIViewController(_ uiViewController: UIViewController, context: Context) {
	}
}

// Conformance to XMTP iOS's SigningKey protocol
class Signer: SigningKey {
	var account: WalletConnectUtils.Account
	var session: WalletConnectSign.Session

	var address: String {
		account.address
	}

	init(session: WalletConnectSign.Session, account: WalletConnectUtils.Account) {
		self.session = session
		self.account = account
		self.cancellable = Sign.instance.sessionResponsePublisher.sink { response in
			guard case let .response(codable) = response.result else {
				return
			}

			// swiftlint:disable force_cast
			let signatureData = Data(hexString: codable.value as! String)
			// swiftlint:enable force_cast
			let signature = Signature(bytes: signatureData[0..<64], recovery: Int(signatureData[64]))

			self.continuation?.resume(returning: signature)
			self.continuation = nil
		}
	}

	var cancellable: AnyCancellable?
	var continuation: CheckedContinuation<Signature, Never>?

	func sign(_ data: Data) async throws -> Signature {
		let address = account.address
		let topic = session.topic
		let blockchain = account.blockchain

		return await withCheckedContinuation { continuation in
			self.continuation = continuation

			Task {
				let method = "personal_sign"
				let walletAddress = address
				let requestParams = AnyCodable([
					String(data: data, encoding: .utf8),
					walletAddress
				])

				let request = Request(
					topic: topic,
					method: method,
					params: requestParams,
					chainId: blockchain
				)

				try await Sign.instance.request(params: request)
			}
		}
	}

	func sign(message: String) async throws -> Signature {
		try await sign(Data(message.utf8))
	}
}

struct LoginView: View {
	var onConnected: (Client) -> Void
	var publishers: [AnyCancellable] = []

	@State private var isShowingWebview = true
	// swiftlint:disable function_body_length
	init(
		onConnected: @escaping (Client) -> Void
	) {
		self.onConnected = onConnected

		Networking.configure(
			projectId: "YOUR PROJECT ID",
			socketFactory: SocketFactory()
		)

		WalletConnectModal.configure(
			projectId: "YOUR PROJECT ID",
			metadata: .init(
				name: "XMTP Chat",
				description: "It's a chat app.",
				url: "https://localhost:4567",
				icons: [],
				redirect: AppMetadata.Redirect(
					native: "",
					universal: nil
				)
			)
		)

		let requiredNamespaces: [String: ProposalNamespace] = [:]
		let optionalNamespaces: [String: ProposalNamespace] = [
			"eip155": ProposalNamespace(
				chains: [
					// swiftlint:disable force_unwrapping
					Blockchain("eip155:80001")!,        // Polygon Testnet
					Blockchain("eip155:421613")!        // Arbitrum Testnet
					// swiftlint:enable force_unwrapping
				],
				methods: [
					"personal_sign"
				], events: []
			)
		]

		WalletConnectModal.set(sessionParams: .init(
				requiredNamespaces: requiredNamespaces,
				optionalNamespaces: optionalNamespaces,
				sessionProperties: nil
		))

		Sign.instance.sessionSettlePublisher
			.receive(on: DispatchQueue.main)
			.sink { session in
				guard let account = session.accounts.first else { return }

				Task(priority: .high) {
					let signer = Signer(session: session, account: account)
					let client = try await Client.create(
						account: signer,
						options: .init(
							api: .init(env: .local, isSecure: false),
							codecs: [GroupUpdatedCodec()],
							enableV3: true
						)
					)

					await MainActor.run {
						onConnected(client)
					}
				}

				print("GOT AN ACCOUNT \(account)")
			}
			.store(in: &publishers)
	}
	// swiftlint:enable function_body_length

	var body: some View {
		ModalWrapper()
	}
}

#Preview {
	LoginView(onConnected: { _ in })
}
