import Foundation
import Security
import SwiftUI
import XmtpSdk

struct PreviewClientProvider<Content: View>: View {
	@State private var client: SDKClient?
	@State private var error: String?
	var content: (SDKClient) -> Content

	init(@ViewBuilder _ content: @escaping (SDKClient) -> Content) {
		self.content = content
	}

	var body: some View {
		Group {
			if let client {
				content(client)
			} else if let error {
				Text(error)
			} else {
				ProgressView("Creating client")
			}
		}.task {
			do {
				let signer = await generateLocalSigner()
				client = try await SDKClient.create(
					signer: signer,
					options: ClientOptions(
						backend: .options(options: BackendOptions(url: exampleBackendUrl)),
						storage: StorageOptions(location: .inMemory),
					),
				)
			} catch { self.error = error.localizedDescription }
		}.onDisappear {
			let closing = client
			client = nil
			Task { try? await closing?.end() }
		}
	}
}

func secureRandomBytes(count: Int) throws -> Data {
	var bytes = [UInt8](repeating: 0, count: count)
	let status = SecRandomCopyBytes(kSecRandomDefault, count, &bytes)
	guard status == errSecSuccess else { throw NSError(domain: NSOSStatusErrorDomain, code: Int(status)) }
	return Data(bytes)
}
