import SwiftUI

struct GroupFieldsFeatureView: View {
	let graph: AppGraph
	let id: String
	var body: some View {
		ContentUnavailableView(
			"Group fields",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Group fields")
	}
}
