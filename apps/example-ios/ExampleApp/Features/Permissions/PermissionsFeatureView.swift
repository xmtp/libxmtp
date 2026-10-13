import SwiftUI

struct PermissionsFeatureView: View {
	let graph: AppGraph
	let id: String
	var body: some View {
		ContentUnavailableView(
			"Permissions",
			systemImage: "bubble.left.and.bubble.right",
			description: Text("This screen is added in the next implementation stage."),
		).navigationTitle("Permissions")
	}
}
