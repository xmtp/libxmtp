//
//  ContentView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 11/22/22.
//

import SwiftUI
import XMTP

struct ContentView: View {
	enum Status {
		case unknown, connecting, connected(Client), error(String)
	}

	@StateObject var accountManager = AccountManager()

	@State private var status: Status = .unknown

	@State private var isShowingQRCode = false
	@State private var qrCodeImage: UIImage?

	@State private var client: Client?

	var body: some View {
		VStack {
			switch status {
			case .unknown:
				Button("Connect Wallet", action: connectWallet)
				Button("Generate Wallet", action: generateWallet)
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

	func connectWallet() {
		status = .connecting

		do {
			switch try accountManager.account.preferredConnectionMethod() {
			case let .qrCode(image):
				qrCodeImage = image

				DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(1)) {
					self.isShowingQRCode = true
				}
			case let .redirect(url):
				UIApplication.shared.open(url)
			case let .manual(url):
				print("Open \(url) in mobile safari")
			}

			Task {
				do {
					try await accountManager.account.connect()

					for _ in 0 ... 30 {
						if accountManager.account.isConnected {
							let client = try await Client.create(account: accountManager.account)

							let keysData = try client.privateKeyBundle.serializedData()
							Persistence().saveKeys(keysData)

							self.status = .connected(client)
							self.isShowingQRCode = false
							return
						}

						try await Task.sleep(for: .seconds(1))
					}

					self.status = .error("Timed out waiting to connect (30 seconds)")
				} catch {
					await MainActor.run {
						self.status = .error("Error connecting: \(error)")
						self.isShowingQRCode = false
					}
				}
			}
		} catch {
			status = .error("No acceptable connection methods found \(error)")
		}
	}

	func generateWallet() {
		Task {
			do {
				let wallet = try PrivateKey.generate()
				let client = try await Client.create(account: wallet)

				let keysData = try client.privateKeyBundle.serializedData()
				Persistence().saveKeys(keysData)

				await MainActor.run {
					self.status = .connected(client)
				}
			} catch {
				await MainActor.run {
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
