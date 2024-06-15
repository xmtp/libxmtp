//
//  ContentView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 11/22/22.
//

import SwiftUI
import XMTPiOS

struct ContentView: View {
	enum Status {
		case unknown, connecting, connected(Client), error(String)
	}

	@StateObject var accountManager = AccountManager()

	@State private var status: Status = .unknown

	@State private var isShowingQRCode = false
	@State private var qrCodeImage: UIImage?
	@State private var isConnectingWallet = false

	@State private var client: Client?

	var body: some View {
		VStack {
			switch status {
			case .unknown:
				Button("Connect Wallet") { isConnectingWallet = true }
					.sheet(isPresented: $isConnectingWallet) {
						LoginView(onConnected: { client in
							do {
								let keysData = try client.privateKeyBundle.serializedData()
								Persistence().saveKeys(keysData)
								self.status = .connected(client)
							} catch {
								print("Error setting up client: \(error)")
							}

							print("Got a client: \(client)")
						})
					}
				Button("Generate Wallet") { generateWallet() }
				Button("Load Saved Keys") {
					Task {
						do {
							if let keysData = Persistence().loadKeys() {
								let keys = try PrivateKeyBundle(serializedData: keysData)
								let client = try await Client.from(
									bundle: keys,
									options: .init(
										api: .init(env: .local, isSecure: false),
										codecs: [GroupUpdatedCodec()],
										enableV3: true
									)
								)
								await MainActor.run {
									self.status = .connected(client)
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
			if let qrCodeImage = qrCodeImage {
				QRCodeSheetView(image: qrCodeImage)
			}
		}
	}

	func generateWallet() {
		Task {
			do {
				let wallet = try PrivateKey.generate()
				let client = try await Client.create(
					account: wallet,
					options: .init(
						api: .init(env: .local, isSecure: false, appVersion: "XMTPTest/v1.0.0"),
						codecs: [GroupUpdatedCodec()],
						enableV3: true
					)
				)

				let keysData = try client.privateKeyBundle.serializedData()
				Persistence().saveKeys(keysData)

				await MainActor.run {
					self.status = .connected(client)
				}
			} catch {
				await MainActor.run {
					print("ERROR: \(error.localizedDescription)")
					self.status = .error("Error generating wallet: \(error)")
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
