import SwiftUI
import UIKit

struct PushSettingsSection: View {
	let coordinator: PushCoordinator?
	var body: some View {
		Section {
			#if PUSH_ENABLED
				Toggle(isOn: Binding(get: { coordinator?.enabled ?? false }, set: { coordinator?.setEnabled($0) })) {
					Text("Notifications", tableName: "Push")
				}
			#endif
			LabeledContent {
				Text(String(
					localized: String.LocalizationValue(coordinator?.status ?? "Off: APNs is not configured"),
					table: "Push",
				))
			} label: { Text("Status", tableName: "Push") }
			#if PUSH_ENABLED
				if coordinator?.status == "Off: iOS permission is required" {
					Button(String(localized: "Open Settings", table: "Push")) {
						if let url = URL(string: UIApplication.openSettingsURLString) {
							UIApplication.shared.open(url)
						}
					}
					.buttonStyle(.glass)
				}
			#endif
		} header: { Text("Notifications", tableName: "Push") } footer: {
			Text("push.explanation", tableName: "Push")
		}
	}
}
