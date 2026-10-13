import SwiftUI

struct AttachmentRecoveryFeatureView: View {
	let graph: AppGraph

	var body: some View {
		ContentUnavailableView(
			"Attachment recovery",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Attachment recovery")
	}
}
