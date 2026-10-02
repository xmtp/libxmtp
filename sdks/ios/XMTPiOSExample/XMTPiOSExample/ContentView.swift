import SwiftUI
import XmtpSdk

let exampleBackendUrl = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050"

func exampleOptions(key: Data) -> ClientOptions {
	ClientOptions(
		backend: .options(options: BackendOptions(url: exampleBackendUrl)),
		storage: StorageOptions(location: .default, encryptionKey: key),
	)
}

struct ContentView: View {
	@State private var client: SDKClient?
	@State private var error: String?
	@State private var isConnecting = false

	var body: some View {
		VStack {
			if let client {
				LoggedInView(client: client)
				Button("Disconnect") {
					Task {
						do {
							try await client.end()
							self.client = nil
						} catch { self.error = error.localizedDescription }
					}
				}
			} else {
				Button("Generate wallet") { connect(reopen: false) }
				Button("Load saved keys") { connect(reopen: true) }
			}
			if isConnecting {
				ProgressView("Connecting")
			}
			if let error {
				Text(error).foregroundStyle(.red)
			}
		}.disabled(isConnecting)
	}

	private func connect(reopen: Bool) {
		Task {
			isConnecting = true
			defer { isConnecting = false }
			do {
				let persistence = Persistence()
				if reopen {
					guard let key = persistence.loadKeys(), let address = persistence.loadAddress() else {
						error = "No saved keys"
						return
					}
					client = try await SDKClient.build(
						identity: PublicIdentity(identifier: address, kind: .ethereum),
						options: exampleOptions(key: key),
					)
				} else {
					let signer = await generateLocalSigner()
					let key = try secureRandomBytes(count: 32)
					let created = try await SDKClient.create(signer: signer, options: exampleOptions(key: key))
					persistence.saveKeys(key)
					persistence.saveAddress(created.identity().identifier)
					client = created
				}
				error = nil
			} catch { self.error = error.localizedDescription }
		}
	}
}
