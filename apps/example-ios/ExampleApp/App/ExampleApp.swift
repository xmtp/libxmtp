import SwiftUI

@main struct ExampleApp: App {
	@State private var graph = AppGraph()
	var body: some Scene {
		WindowGroup { RootView(graph: graph) }
	}
}
