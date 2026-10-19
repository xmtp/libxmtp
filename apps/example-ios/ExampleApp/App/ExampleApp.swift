import SwiftUI

@main struct ExampleApp: App {
	@UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
	@State private var graph: AppGraph?
	@State private var failure: String?
	/// The app-level phase is active while any window is active.
	@Environment(\.scenePhase) private var scenePhase
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
						.task { do {
							let graph = try AppDelegate.ensureGraph(); self.graph = graph
							await graph.setSceneActive(scenePhase == .active)
						} catch { failure = String(
							localized: "Cannot open local account storage.",
							table: "Foundation",
						) } }
				}
			}
		}
		.onChange(of: scenePhase, initial: true) { _, value in
			Task { await graph?.setSceneActive(value == .active) }
		}
	}
}
