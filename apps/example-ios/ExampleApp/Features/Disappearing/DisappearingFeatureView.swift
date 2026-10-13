import SwiftUI

struct DisappearingFeatureView: View {
	let graph: AppGraph
	let id: String
	var body: some View {
		ContentUnavailableView(
			"Disappearing messages",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Disappearing messages")
	}
}
