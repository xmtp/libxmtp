import SwiftUI

@main struct ExampleApp: App {
	@State private var graph: AppGraph?
	@State private var failure: String?
	var body: some Scene {
		WindowGroup {
			Group {
				if let graph {
					RootView(graph: graph)
				} else if let failure {
					ContentUnavailableView("Cannot open app", systemImage: "exclamationmark.triangle", description: Text(failure))
				} else {
					ProgressView()
						.task {
							do { try UITestHooks.resetIfRequested(); graph = try AppGraph() } catch {
								failure = "Cannot open local account storage."
							}
						}
				}
			}
		}
	}
}
