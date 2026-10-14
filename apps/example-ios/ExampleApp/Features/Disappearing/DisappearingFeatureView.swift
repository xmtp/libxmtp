import ExampleCore
import SwiftUI

struct DisappearingFeatureView: View {
	var preview = false
	let graph: AppGraph
	let id: String
	@State private var store: DetailsStore?
	@State private var seconds = "0"
	@State private var custom = false
	@State private var error: String?
	private let presets: [(String, String)] = [
		("Off", "0"),
		("1 hour", "3600"),
		("8 hours", "28800"),
		("1 day", "86400"),
		("1 week", "604800"),
	]
	var body: some View {
		List {
			Section { ForEach(presets, id: \.0) { label, value in Button { custom = false
				seconds = value
				save()
			} label: { HStack {
				Text(disappearingCopy(label))
				Spacer()
				if seconds == value, !custom {
					Image(systemName: "checkmark").accessibilityLabel(disappearingCopy("Selected"))
				}
			} } }
			Button { custom = true } label: { HStack {
				Text(disappearingCopy("Custom"))
				Spacer()
				if custom {
					Image(systemName: "checkmark").accessibilityLabel(disappearingCopy("Selected"))
				}
			} }
			} header: { Text(disappearingCopy("Message duration")) } footer: {
				Text(disappearingCopy("Set how long messages remain in this conversation."))
			}
			if custom {
				Section { TextField(disappearingCopy("Duration in seconds"), text: $seconds).keyboardType(.numberPad)
					Button(disappearingCopy("Save duration"), action: save).buttonStyle(.glassProminent)
				} header: { Text(disappearingCopy("Custom duration")) } footer: { Text(disappearingCopy("Use 0 to turn this off."))
				}
			}
			DetailsErrorView(error: error ?? store?.state.error, table: "Disappearing")
		}.listStyle(.insetGrouped).navigationTitle(disappearingCopy("Disappearing messages"))
			.task(id: graph.routes.token) { store = preview ? previewDetailsStore() : makeDetailsStore(graph, id: id)
				store?.send(.refresh)
			}
			.onChange(of: store?.state.details?.retentionNs) { _, value in
				if let value {
					seconds = String(value / 1_000_000_000)
					custom = !presets.contains { $0.1 == seconds }
				}
			}
	}

	private func save() {
		do { let value = try DisappearingRules.nanoseconds(seconds)
			error = nil
			store?.send(.mutate(.disappearing(value)))
		} catch { self.error = "Enter a whole number of seconds from 0 to 9223372036." }
	}
}

#Preview("Light") { DetailsPreviewHost { graph in DisappearingFeatureView(
	preview: true,
	graph: graph,
	id: "preview-group",
) } }
#Preview("Dark") { DetailsPreviewHost { graph in DisappearingFeatureView(
	preview: true,
	graph: graph,
	id: "preview-group",
) }.preferredColorScheme(.dark) }
#Preview("Large text") { DetailsPreviewHost { graph in DisappearingFeatureView(
	preview: true,
	graph: graph,
	id: "preview-group",
) }.dynamicTypeSize(.accessibility3) }
