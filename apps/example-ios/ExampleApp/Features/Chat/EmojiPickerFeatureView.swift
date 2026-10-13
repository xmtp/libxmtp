import SwiftUI

struct EmojiPickerFeatureView: View {
	let graph: AppGraph
	let conversationID: String; let messageID: String
	var body: some View {
		ContentUnavailableView(
			"Choose a reaction",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Choose a reaction")
	}
}
