import ExampleCore
import SwiftUI

struct PermissionsFeatureView: View {
	var preview = false
	let graph: AppGraph
	let id: String
	@State private var store: DetailsStore?
	var body: some View {
		List {
			Section {
				preset("All members", detail: "Members can update group details and add people.", adminOnly: false)
				preset("Admins only", detail: "Only admins can update group details and add people.", adminOnly: true)
				HStack(spacing: 12) {
					radio(selected: store?.state.details?.preset == "Custom")
					VStack(alignment: .leading, spacing: 4) {
						Text(permissionsCopy("Custom")).foregroundStyle(Color.primary)
						Text(permissionsCopy("Shown when the policies match no preset."))
							.font(.footnote).foregroundStyle(Theme.secondaryLabel)
					}
				}.opacity(store?.state.details?.preset == "Custom" ? 1 : 0.6).disabled(true)
			} header: { Text(permissionsCopy("Who can change group settings")) } footer: {
				Text(
					permissionsCopy(
						"A preset makes several policy changes. "
							+ "If one change fails, this screen shows the policies that the group has now.",
					),
				)
			}
			Section {
				ForEach(policyRows, id: \.title) { row in
					LabeledContent {
						Text(policyValue(row.writes)).foregroundStyle(Theme.secondaryLabel)
							.multilineTextAlignment(.trailing).fixedSize(horizontal: false, vertical: true)
					} label: { Text(permissionsCopy(row.title)).foregroundStyle(Color.primary) }
				}
			} header: { Text(permissionsCopy("Current policies")) } footer: {
				Text(permissionsCopy("Only super admins can change permissions."))
			}
			if store?.state.busy == true {
				ProgressView()
			}
			DetailsErrorView(error: store?.state.error, table: "Permissions")
		}.listStyle(.insetGrouped).navigationTitle(permissionsCopy("Permissions"))
			.navigationBarTitleDisplayMode(.inline)
			.task(id: graph.routes.token) { store = preview ? previewDetailsStore() : makeDetailsStore(graph, id: id)
				store?.send(.refresh)
			}
	}

	private func preset(_ title: String, detail: String, adminOnly: Bool) -> some View {
		Button { store?.send(.preset(adminOnly)) } label: {
			HStack(spacing: 12) {
				radio(selected: store?.state.details?.preset == title)
				VStack(alignment: .leading, spacing: 4) {
					Text(permissionsCopy(title)).foregroundStyle(Color.primary)
					Text(permissionsCopy(detail)).font(.footnote).foregroundStyle(Theme.secondaryLabel)
				}
				Spacer(minLength: 0)
			}
		}.buttonStyle(.plain).disabled(store?.state.busy == true || store?.canChangePermissionPreset != true)
			.accessibilityAddTraits(store?.state.details?.preset == title ? .isSelected : [])
	}

	private func radio(selected: Bool) -> some View {
		Image(systemName: selected ? "largecircle.fill.circle" : "circle")
			.font(.title3).frame(minWidth: 24)
			.foregroundStyle(selected ? Theme.tint : Theme.secondaryLabel)
			.accessibilityHidden(true)
	}

	private struct PolicyRow { let title: String; let writes: [PermissionWrite] }
	private var policyRows: [PolicyRow] {
		let writes = store?.state.details?.policies ?? []
		return [
			PolicyRow(title: "Add members", writes: writes.filter { $0.kind == .addMember }),
			PolicyRow(title: "Remove members", writes: writes.filter { $0.kind == .removeMember }),
			PolicyRow(title: "Add or remove admins", writes: writes.filter { $0.kind == .addAdmin || $0.kind == .removeAdmin }),
			PolicyRow(
				title: "Name, description, image",
				writes: writes.filter { write in
					guard let field = write.field else { return false }
					return [GroupMetadataKind.name, .description, .imageUrl].contains(field)
				},
			),
			PolicyRow(title: "App data", writes: writes.filter { $0.field == .appData }),
			PolicyRow(title: "Disappearing messages", writes: writes.filter { $0.field == .disappearing }),
		]
	}

	private func policyValue(_ writes: [PermissionWrite]) -> String {
		guard let first = writes.first else { return "—" }
		if writes.dropFirst().allSatisfy({ $0.policy == first.policy }) {
			return permissionsCopy(first.policy.rawValue)
		}
		return writes.map { permissionsCopy(label($0)) + ": " + permissionsCopy($0.policy.rawValue) }.joined(separator: "; ")
	}

	private func label(_ write: PermissionWrite) -> String {
		if let field = write.field {
			return fieldLabel(field)
		}
		switch write.kind {
		case .addMember: return "Add members"
		case .removeMember: return "Remove members"
		case .addAdmin: return "Add admins"
		case .removeAdmin: return "Remove admins"
		case .updateMetadata: return "Metadata"
		}
	}

	private func fieldLabel(_ field: GroupMetadataKind) -> String {
		switch field {
		case .name: "Name"
		case .description: "Description"
		case .imageUrl: "Image"
		case .appData: "App data"
		case .disappearing: "Disappearing messages"
		}
	}
}

#Preview("Light") { DetailsPreviewHost { graph in PermissionsFeatureView(
	preview: true,
	graph: graph,
	id: "preview-group",
) } }
#Preview("Dark") { DetailsPreviewHost { graph in
	PermissionsFeatureView(preview: true, graph: graph, id: "preview-group")
}.preferredColorScheme(.dark) }
#Preview("Large text") { DetailsPreviewHost { graph in PermissionsFeatureView(
	preview: true,
	graph: graph,
	id: "preview-group",
) }.dynamicTypeSize(.accessibility3) }
