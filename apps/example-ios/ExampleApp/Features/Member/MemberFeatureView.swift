import SwiftUI

struct MemberFeatureView: View {
	let graph: AppGraph
	let conversationID: String; let inboxID: String
	var body: some View {
		ContentUnavailableView(
			"Member",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Member")
	}
}
