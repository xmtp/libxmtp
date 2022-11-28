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
		case unknown, connecting, connected, error(String)
	}

	@StateObject var walletManager = WalletManager()

	@State private var status: Status = .unknown

	@State private var isShowingQRCode = false
	@State private var qrCodeImage: UIImage?

	var body: some View {
		VStack {
			switch status {
			case .unknown:
				Button("Connect Wallet", action: connectWallet)
			case .connecting:
				ProgressView("Connecting…")
			case .connected:
				LoggedInView(wallet: walletManager.wallet)
			case let .error(error):
				Text("Error: \(error)").foregroundColor(.red)
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
			switch try walletManager.wallet.preferredConnectionMethod() {
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
					try await walletManager.wallet.connect()

					for _ in 0 ... 30 {
						if walletManager.wallet.connection.isConnected {
							self.status = .connected
							self.isShowingQRCode = false
							break
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
}

struct ContentView_Previews: PreviewProvider {
	static var previews: some View {
		ContentView()
	}
}
