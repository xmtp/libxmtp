import ExampleCore
import SwiftUI

struct MemberFeatureView: View {
	var preview = false
	let graph: AppGraph
	let conversationID: String
	let inboxID: String
	@State private var store: DetailsStore?
	@State private var remove = false
	@State private var removalRequested = false
	var body: some View {
		List {
			if let store, let details = store.state.details, let member = details.members.first(where: { $0.id == inboxID }) {
				Section { HStack(spacing: 14) {
					Avatar(seed: member.id, label: String(member.title.prefix(1)).uppercased(), size: 52)
					VStack(alignment: .leading, spacing: 6) {
						IdentityText(inboxID: member.id, handle: member.handle).font(.title3.weight(.semibold))
						Text(memberCopy(member.role.rawValue)).font(.caption.weight(.medium)).foregroundStyle(.blue)
							.padding(.horizontal, 8).padding(.vertical, 4).background(.blue.opacity(0.1), in: Capsule())
					}
					Spacer()
				}.padding(.vertical, 6) }.listRowBackground(Color.clear)
				Section {
					LabeledContent(
						memberCopy("Other handles"),
						value: member.otherHandles.isEmpty ? "—" : member.otherHandles.joined(separator: ", "),
					)
					Text(member.id).font(.footnote.monospaced()).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
					Button { UIPasteboard.general.string = member.id } label: { Label(
						memberCopy("Copy inbox ID"),
						systemImage: Symbol.copy,
					).frame(maxWidth: .infinity, alignment: .leading) }.buttonStyle(.plain)
				} header: { Text(memberCopy("Inbox ID")) }
				if details.canChangeRole(member, own: graph.active?.profile.inboxID ?? "") || details
					.canRemoveMember(member, own: graph.active?.profile.inboxID ?? "")
				{
					Section {
						if details.canChangeRole(member, own: graph.active?.profile.inboxID ?? "") {
							Button { store.send(.mutate(.admin(
								member.id,
								member.role != .admin,
							))) } label: { Label(
								memberCopy(member.role == .admin ? "Remove admin" : "Make admin"),
								systemImage: "person.badge.key",
							) }.buttonStyle(.plain)
						}
						if details.canRemoveMember(member, own: graph.active?.profile.inboxID ?? "") {
							Button(role: .destructive) { remove = true } label: { Label(
								memberCopy("Remove from group"),
								systemImage: "person.fill.xmark",
							).foregroundStyle(.red) }.buttonStyle(.plain).accessibilityIdentifier("member.remove")
						}
					} footer: {
						Text(memberCopy("Available actions follow the group permissions. Super admins cannot be changed here."))
					}
				}
				if store.state.pendingRoles[inboxID] != nil || store.state.pendingRemoval.contains(inboxID) {
					Text(memberCopy("Membership change pending"))
				}
				DetailsErrorView(error: store.state.error, table: "Member")
			} else if let store, store.state.details != nil, !store.state.busy {
				ContentUnavailableView(memberCopy("Member no longer in this group"), systemImage: "person.crop.circle.badge.xmark")
				DetailsErrorView(error: store.state.error, table: "Member")
			} else if let error = store?.state.initialLoadError {
				DetailsErrorView(error: error, table: "Member")
			} else {
				ProgressView()
			}
		}.listStyle(.insetGrouped).navigationTitle(memberCopy("Member"))
			.navigationBarTitleDisplayMode(.inline)
			.toolbar { ToolbarItem(placement: .cancellationAction) {
				Button(memberCopy("Close"), systemImage: "xmark") { graph.routes.back() }
					.labelStyle(.iconOnly).buttonStyle(.plain).accessibilityIdentifier("member.close")
			} }
			.task(id: graph.routes.token) { store = preview ? previewDetailsStore() : makeDetailsStore(graph, id: conversationID)
				store?.send(.refresh)
			}
			.onChange(of: store?.state) { _, state in
				guard removalRequested, let state, !state.busy, state.error == nil, let details = state.details,
				      !details.members.contains(where: { $0.id == inboxID }),
				      !state.pendingRemoval.contains(inboxID) else { return }
				graph.routes.dismissSheet()
			}
			.confirmationDialog(memberCopy("Remove from group?"), isPresented: $remove) { Button(
				memberCopy("Remove from group"),
				role: .destructive,
			) { removalRequested = true; store?.send(.mutate(.remove(inboxID))) }
				.accessibilityIdentifier("member.confirm-remove")
			}
	}
}

#Preview("Light") { DetailsPreviewHost { graph in MemberFeatureView(
	preview: true,
	graph: graph,
	conversationID: "preview-group",
	inboxID: "priya",
) } }
#Preview("Dark") { DetailsPreviewHost { graph in MemberFeatureView(
	preview: true,
	graph: graph,
	conversationID: "preview-group",
	inboxID: "priya",
) }.preferredColorScheme(.dark) }
#Preview("Large text") { DetailsPreviewHost { graph in MemberFeatureView(
	preview: true,
	graph: graph,
	conversationID: "preview-group",
	inboxID: "priya",
) }.dynamicTypeSize(.accessibility3) }
