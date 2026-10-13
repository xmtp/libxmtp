import SwiftUI

struct ReactionsFeatureView: View {
	let graph: AppGraph
	let conversationID: String; let messageID: String; let emoji: String
	var body: some View {
		ContentUnavailableView(
			"Reactions",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Reactions")
	}
}
