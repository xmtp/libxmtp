//
//  ContentView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 11/22/22.
//

import SwiftUI
import XMTPiOS

/// The backend this sample app connects to.
///
/// `localhost` is the device or the simulator, not the development machine, so
/// a real device needs the machine's address on the local network here (for
/// example `http://192.168.1.10:5050`). The simulator shares the host network
/// and reaches a local `just backend up` stack at this default.
let exampleBackendUrl = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"]
	?? "http://localhost:5050"

struct ContentView: View {
	enum Status {
		case unknown, connecting, connected(Client), error(String)
	}

	@State private var status: Status = .unknown

	@State private var isShowingQRCode = false
	@State private var qrCodeImage: UIImage?
	@State private var isConnectingWallet = false

	@State private var client: Client?

	var body: some View {
		VStack {
			switch status {
			case .unknown:
				Button("Generate Wallet") { generateWallet() }
				Button("Load Saved Keys") {
					Task {
						do {
							if let keysData = Persistence().loadKeys() {
								if let address = Persistence().loadAddress() {
									let client = try await Client.build(
										publicIdentity: PublicIdentity(kind: IdentityKind.ethereum, identifier: address),
										options: .init(
											api: .init(backendUrl: exampleBackendUrl),
											codecs: [GroupUpdatedCodec()],
											dbEncryptionKey: keysData
										)
									)
									await MainActor.run {
										status = .connected(client)
									}
								}
							}
						} catch {
							print("Error loading keys \(error)")
						}
					}
				}
			case .connecting:
				ProgressView("Connecting…")
			case let .connected(client):
				LoggedInView(client: client)
			case let .error(error):
				Text("Error: \(error)").foregroundColor(.red)
			}
		}
		.task {
			UIApplication.shared.registerForRemoteNotifications()

			do {
				_ = try await XMTPPush.shared.request()
			} catch {
				print("Error requesting push access: \(error)")
			}
		}
		.sheet(isPresented: $isShowingQRCode) {
			if let qrCodeImage {
				QRCodeSheetView(image: qrCodeImage)
			}
		}
	}

	func generateWallet() {
		Task {
			do {
				let wallet = try PrivateKey.generate()
				let key = try secureRandomBytes(count: 32)
				Persistence().saveKeys(key)
				Persistence().saveAddress(wallet.identity.identifier)
				let client = try await Client.create(
					account: wallet,
					options: .init(
						api: .init(backendUrl: exampleBackendUrl),
						codecs: [GroupUpdatedCodec()],
						dbEncryptionKey: key
					)
				)

				await MainActor.run {
					status = .connected(client)
				}
			} catch {
				await MainActor.run {
					print("ERROR: \(error.localizedDescription)")
					status = .error("Error generating wallet: \(error)")
				}
			}
		}
	}
}

struct ContentView_Previews: PreviewProvider {
	static var previews: some View {
		ContentView()
	}
}
