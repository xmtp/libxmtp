import Foundation
import Security

/// Test reset is absent from Release and Push binaries.
@MainActor enum UITestHooks {
	static func resetIfRequested() throws {
		#if DEBUG
			guard ProcessInfo.processInfo.arguments.contains("-reset-all-data") else { return }
			let files = try ProfileFiles()
			try files.resetAllForTesting()
			try KeychainSecretStore().resetAllForTesting()
		#endif
	}
}
