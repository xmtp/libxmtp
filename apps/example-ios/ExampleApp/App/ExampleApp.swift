import SwiftUI

@main struct ExampleApp: App {
	@UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
	@State private var graph: AppGraph?
	@State private var failure: String?
	var body: some Scene {
		WindowGroup {
			Group {
				if let graph {
					RootView(graph: graph)
				} else if let failure {
					ContentUnavailableView {
						Label(String(localized: "Cannot open app", table: "Foundation"), systemImage: "exclamationmark.triangle")
					} description: {
						Text(failure)
					} actions: {
						Button(String(localized: "Retry", table: "Foundation")) { self.failure = nil }
							.buttonStyle(.glass)
							.accessibilityIdentifier("startup.retryStorage")
					}
				} else {
					ProgressView()
						.task { do { graph = try AppDelegate.ensureGraph() } catch { failure = String(
							localized: "Cannot open local account storage.",
							table: "Foundation",
						) } }
				}
			}
		}
	}
}
