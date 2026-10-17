import ExampleCore
import SwiftUI

struct DetailsFeatureView: View {
	var preview = false
	let graph: AppGraph
	let id: String
	@State private var store: DetailsStore?
	@State private var alias = ""
	@State private var addMember = false
	@State private var edit = false
	@State private var name = ""
	@State private var description = ""
	@State private var confirmation: String?
	@State private var waitingForBlock = false
	@Environment(\.dynamicTypeSize) private var dynamicTypeSize
	@ViewBuilder private func trailingSections(_ store: DetailsStore, details: ConversationDetails) -> some View {
		Section {
			if details.record.kind == .group {
				Button { graph.navigate(.groupFields(id)) } label: { DetailsNavigationRow(
					title: detailsCopy("Group fields"),
					symbol: "curlybraces",
				) }
			}; Button { graph.navigate(.myFields(id)) } label: { DetailsNavigationRow(
				title: detailsCopy("My fields"),
				symbol: "person.crop.square",
			) }
		} header: { Text(detailsCopy("Developer tools")) } footer: {
			Text(detailsCopy("Raw metadata fields for this conversation."))
		}
		Section {
			Button(detailsCopy("Block conversation"), role: .destructive) { confirmation = "block" }.buttonStyle(.plain)
			if details.canLeave(own: graph.active?.profile.inboxID ?? "") {
				Button(detailsCopy("Leave group"), role: .destructive) { confirmation = "leave" }.buttonStyle(.plain)
			}
			if store.state.pendingLeave {
				Text(detailsCopy("Membership change pending"))
			}
		} footer: {
			Text(
				detailsCopy(
					"details.localRemovalFootnote",
				),
			)
		}
		DetailsErrorView(error: store.state.error)
	}

	private func header(_ details: ConversationDetails) -> some View {
		VStack(spacing: 10) {
			Avatar(seed: id, label: details.avatarLabel(alias: alias), size: 96)
			ConversationTitle(title: details.record.title(alias: alias)).font(.title.bold())
			Text(details.record.kind == .group ? "Group · \(details.members.count) members" : details.record.peerHandle
				.map { "@" + $0 + " · Direct message" } ?? detailsCopy("Direct message")).font(.subheadline)
				.foregroundStyle(.secondary)
			if !details.description.isEmpty {
				Text(details.description).foregroundStyle(.secondary)
			}
			shortcutLayout {
				#if PUSH_ENABLED
					Button { graph.push?.setConversationEnabled(
						graph.preferences?
							.muted(for: details.record.logicalKey(ownInboxID: graph.active?.profile.inboxID ?? "")) == true,
						id: id,
					) } label: { Label(
						graph.preferences?
							.muted(for: details.record.logicalKey(ownInboxID: graph.active?.profile.inboxID ?? "")) == true ? "Unmute" :
							"Mute",
						systemImage: "bell.slash",
					) }.buttonStyle(.plain)
				#endif
				if details.record.kind == .group {
					Button { addMember = true } label: { Label(detailsCopy("Add member"), systemImage: Symbol.addMember) }
						.buttonStyle(.plain).accessibilityIdentifier("details.add-member-shortcut")
				}
				Button { graph.navigate(.disappearing(id)) } label: { Label(
					detailsCopy("Disappearing"),
					systemImage: Symbol.disappearing,
				) }.buttonStyle(.plain)
			}.labelStyle(DetailsShortcutLabelStyle()).buttonStyle(.plain).padding(.top, 6)
		}.frame(maxWidth: .infinity)
	}

	private var shortcutLayout: AnyLayout {
		dynamicTypeSize.isAccessibilitySize
			? AnyLayout(VStackLayout(spacing: 10))
			: AnyLayout(HStackLayout(spacing: 10))
	}

	var body: some View {
		List {
			if let store, let details = store.state.details {
				Section {
					header(details)
				}.listRowBackground(Color.clear)
					.listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 0, trailing: 0))
				Section { TextField(detailsCopy("Local name"), text: $alias).onSubmit { saveAlias() } } header: {
					Text(detailsCopy("Name on this device"))
				} footer: {
					Text(detailsCopy("Use a local name to find this conversation. Other people do not see this name."))
				}
				#if PUSH_ENABLED
					Section { Toggle(
						detailsCopy("Notifications"),
						isOn: Binding(
							get: {
								graph.preferences?
									.muted(for: details.record.logicalKey(ownInboxID: graph.active?.profile.inboxID ?? "")) != true
							},
							set: { graph.push?.setConversationEnabled($0, id: id) },
						),
					) } footer: { Text("Get alerts for new messages in this conversation.") }
				#else
					Section { Text(detailsCopy("Notifications are not configured in this build.")) }
				#endif
				Section {
					if details.record.kind == .group {
						Button { addMember = true } label: { Label(
							detailsCopy("Add member"),
							systemImage: Symbol.addMember,
						) }.tint(.blue).accessibilityIdentifier("details.add-member-row")
					}
					ForEach(details.members) { member in
						Button { graph.navigate(.member(conversationID: id, inboxID: member.id)) } label: {
							MemberIdentityRow(member: member)
						}.tint(.primary).accessibilityIdentifier("details.member." + member.id)
						if store.state.pendingRoles[member.id] != nil || store.state.pendingRemoval.contains(member.id) {
							Text(detailsCopy("Membership change pending")).font(.caption).foregroundStyle(.secondary)
						}
					}
					ForEach(Array(store.state.pendingMembers.values).sorted { $0.id < $1.id }) { member in
						HStack { MemberIdentityRow(member: member); Text(detailsCopy("Pending")).font(.caption) }
					}
				} header: { Text("\(detailsCopy("Members")) · \(details.members.count)") } footer: {
					Text(detailsCopy("Membership changes stay pending until the group confirms them."))
				}
				Section(detailsCopy("Shared media and files")) { Button { graph.navigate(.sharedMedia(id)) } label: { Label(
					detailsCopy("Media and files"),
					systemImage: Symbol.photo,
				) } }
				Section(detailsCopy("Group settings")) {
					if details.record.kind == .group {
						Button { graph.navigate(.permissions(id)) } label: { DetailsNavigationRow(
							title: detailsCopy("Permissions"),
							symbol: "lock.shield",
							value: detailsCopy(details.preset),
						) }
					}
					Button { graph.navigate(.disappearing(id)) } label: { DetailsNavigationRow(
						title: detailsCopy("Disappearing messages"),
						symbol: Symbol.disappearing,
						value: details.retentionNs == 0 ? detailsCopy("Off") : "\(details.retentionNs / 1_000_000_000) s",
					) }
				}
				trailingSections(store, details: details)
			} else if let error = store?.state.initialLoadError {
				DetailsErrorView(error: error)
				Button(detailsCopy("Retry")) { store?.send(.refresh) }.buttonStyle(.glassProminent)
			} else {
				ProgressView()
			}
		}.listStyle(.insetGrouped).contentMargins(.top, 0, for: .scrollContent)
			.navigationTitle(detailsCopy("Details")).navigationBarTitleDisplayMode(.inline)
			.toolbar {
				ToolbarItem(placement: .principal) {
					Text(detailsCopy("Details")).foregroundStyle(.clear)
				}
				if store?.state.details?.record.kind == .group {
					ToolbarItem(placement: .topBarTrailing) {
						Button {
							name = store?.state.details?.record.name ?? ""; description = store?.state.details?
								.description ?? ""; edit = true
						} label: {
							if dynamicTypeSize.isAccessibilitySize {
								Image(systemName: "pencil").accessibilityLabel(detailsCopy("Edit"))
							} else {
								Text(detailsCopy("Edit"))
							}
						}.buttonStyle(.plain)
					}
				}
			}
			.task(id: graph.routes.token) {
				store = preview ? previewDetailsStore() : makeDetailsStore(graph, id: id); alias = graph.preferences?
					.alias(for: id) ?? ""; store?.send(.refresh)
			}
			.onDisappear { saveAlias() }
			.onChange(of: store?.state.blocked) { _, blocked in
				if blocked == true {
					graph.routes.replace(.chats)
				}
			}
			.onChange(of: store?.state.busy) { _, busy in
				if waitingForBlock, busy == false {
					waitingForBlock = false; if store?.state.error == nil {
						graph.routes.replace(.chats)
					}
				}
			}
			.sheet(isPresented: $addMember) {
				if let store {
					AddMemberSheet(graph: graph, id: id, store: store) { addMember = false }
				}
			}
			.sheet(isPresented: $edit) { NavigationStack { Form { TextField(detailsCopy("Group name"), text: $name); TextField(
				detailsCopy("Description"),
				text: $description,
				axis: .vertical,
			) }.navigationTitle(detailsCopy("Edit group"))
				.toolbar {
					ToolbarItem(placement: .cancellationAction) { Button(detailsCopy("Cancel")) { edit = false }
					}; ToolbarItem(placement: .confirmationAction) { Button(detailsCopy("Save")) { store?.send(.mutate(.edit(
						name,
						description,
					))); edit = false }.buttonStyle(.glassProminent) }
				}
			} }
			.confirmationDialog(
				detailsCopy(confirmation == "leave" ? "Leave group?" : "Block conversation?"),
				isPresented: Binding(get: { confirmation != nil }, set: {
					if !$0 {
						confirmation = nil
					}
				}),
			) {
				Button(detailsCopy(confirmation == "leave" ? "Leave group" : "Block conversation"), role: .destructive) {
					let block = confirmation != "leave"; store?.send(.mutate(block ? .block : .leave)); waitingForBlock = block
				}
			}
	}

	private func saveAlias() {
		do { try graph.preferences?.setAlias(alias, for: id) } catch {
			graph.error = detailsCopy("Cannot save local name. Try again.")
		}
	}
}

#Preview("Light") { DetailsPreviewHost { graph in
	DetailsFeatureView(preview: true, graph: graph, id: "preview-group")
} }
#Preview("Dark") { DetailsPreviewHost { graph in
	DetailsFeatureView(preview: true, graph: graph, id: "preview-group")
}.preferredColorScheme(.dark) }
#Preview("Large text") { DetailsPreviewHost { graph in DetailsFeatureView(
	preview: true,
	graph: graph,
	id: "preview-group",
) }.dynamicTypeSize(.accessibility3) }

private struct DetailsShortcutLabelStyle: LabelStyle {
	func makeBody(configuration: Configuration) -> some View {
		VStack(spacing: 7) { configuration.icon.font(.title3); configuration.title.font(.caption.weight(.semibold)) }
			.foregroundStyle(Theme.accentText)
			.padding(.horizontal, 8).padding(.vertical, 12)
			.frame(maxWidth: .infinity, minHeight: 68)
			.messengerGlass(in: .rect(cornerRadius: 20))
	}
}

private struct DetailsNavigationRow: View {
	let title: String
	let symbol: String
	var value = ""
	var body: some View {
		HStack(spacing: 12) {
			Image(systemName: symbol).foregroundStyle(.blue).frame(width: 24)
			Text(title).foregroundStyle(.primary)
			Spacer()
			if !value.isEmpty {
				Text(value).foregroundStyle(.secondary)
			}
			Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
		}
	}
}
