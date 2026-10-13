import SwiftUI

struct ChatFeatureView: View {
	let graph: AppGraph
	let id: String
	var body: some View {
		ContentUnavailableView(
			"Chat",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Chat")
	}
}
