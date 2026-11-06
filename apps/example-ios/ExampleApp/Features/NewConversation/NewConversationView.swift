import ExampleCore
import SwiftUI

struct NewConversationView: View {
	let store: NewConversationStore
	var onClose: () -> Void = {}
	@Environment(\.motionPolicy) private var motion
	var body: some View {
		Form { people.disabled(store.state.busy); loadingAndError }
			.interactiveDismissDisabled(store.state.busy)
			.contentMargins(.top, 0, for: .scrollContent)
			.listSectionSpacing(16)
			.toolbar {
				ToolbarItem(placement: .topBarLeading) {
					Button(action: onClose) { Image(systemName: "xmark") }
						.accessibilityLabel(localized("Cancel"))
						.accessibilityIdentifier("newConversation.cancel")
						.disabled(store.state.busy)
				}
				ToolbarItem(placement: .principal) { stepTitle(reviewing: false) }
				ToolbarItem(placement: .topBarTrailing) {
					Button(localized(store.state.group ? "Next" : "Create")) { store.send(.next) }.buttonStyle(.glassProminent)
						.disabled(!store.state.canContinue)
						.accessibilityIdentifier("newConversation.next")
				}
			}
			.navigationBarTitleDisplayMode(.inline)
			.navigationDestination(isPresented: Binding(get: { store.state.reviewing }, set: {
				if !$0 {
					store.send(.back)
				}
			})) {
				Form { review; loadingAndError }
					.contentMargins(.top, 12, for: .scrollContent)
					.listSectionSpacing(16)
					.navigationBarBackButtonHidden()
					.toolbar {
						ToolbarItem(placement: .topBarLeading) {
							Button { store.send(.back) } label: { Image(systemName: "chevron.left") }
								.accessibilityLabel(localized("Back"))
								.accessibilityIdentifier("newConversation.back")
								.disabled(store.state.busy)
						}
						ToolbarItem(placement: .principal) { stepTitle(reviewing: true) }
						ToolbarItem(placement: .topBarTrailing) {
							Button(localized("Create")) { store.send(.create) }.buttonStyle(.glassProminent)
								.accessibilityIdentifier("newConversation.create")
								.disabled(!store.state.canContinue)
						}
					}
					.navigationBarTitleDisplayMode(.inline)
			}
			.task { store.send(.load) }
	}

	private func stepTitle(reviewing: Bool) -> some View {
		VStack(spacing: 2) {
			Text(localized(reviewing && store.state.group ? "New group" : "New message")).font(.headline)
			if store.state.group {
				Text(localized(reviewing ? "Step 2 of 2" : "Step 1 of 2")).font(.caption).foregroundStyle(Theme.secondaryLabel)
			}
		}
	}

	@ViewBuilder private var loadingAndError: some View {
		if store.state.busy || store.state.loadingPeople {
			ProgressView().frame(maxWidth: .infinity)
		}
		if let error = store.state.bootstrapError {
			VStack(alignment: .leading, spacing: 8) {
				Text(error).foregroundStyle(.red)
				Button(localized("Retry")) { store.send(.load) }.buttonStyle(.glass)
					.disabled(store.state.busy || store.state.loadingPeople)
					.accessibilityIdentifier("newConversation.retryBootstrap")
			}
		}
		if store.state.reviewing, let error = store.state.error {
			Text(error).foregroundStyle(.red)
		}
	}

	@ViewBuilder
	private var people: some View {
		Picker(
			localized("Conversation type"),
			selection: Binding(get: { store.state.group }, set: { store.send(.group($0)) }),
		) {
			Text("Direct message", tableName: "NewConversation").tag(false)
			Text("Group", tableName: "NewConversation").tag(true)
		}.pickerStyle(.segmented).listRowBackground(Color.clear).listRowInsets(EdgeInsets())
		Section {
			RecipientFlow {
				Text("To:", tableName: "NewConversation").foregroundStyle(Theme.secondaryLabel)
				ForEach(store.state.recipients) { person in
					HStack(spacing: 5) {
						IdentityText(inboxID: person.inboxID, handle: person.handle)
							.accessibilityLabel(person.label)
							.accessibilityIdentifier("recipient.\(person.inboxID)")
						Button { store.send(.remove(person.id)) } label: { Image(systemName: "xmark").font(.caption) }.buttonStyle(.glass)
							.accessibilityLabel("Remove " + person.label)
					}.padding(.vertical, 6).padding(.horizontal, 10).background(Theme.tint.opacity(0.1), in: .capsule)
				}
			}
			HStack {
				TextField(
					localized("Handle, Ethereum address, or inbox ID"),
					text: Binding(get: { store.state.input }, set: { store.send(.input($0)) }),
				).textInputAutocapitalization(.never).autocorrectionDisabled().onSubmit { store.send(.add) }
				Button(localized("Add")) { store.send(.add) }.buttonStyle(.glass)
					.disabled(store.state.input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.state.busy)
			}
		} footer: {
			VStack(alignment: .leading, spacing: 6) {
				if let error = store.state.error {
					Text(error).foregroundStyle(.red)
				}
				Text(localized(store.state
						.group ? "Separate multiple people with commas." : "Enter one handle, Ethereum address, or inbox ID."))
				if store.state.group, store.state.maximumMembers > 0 {
					Text("A handle can start with @. Up to " + String(max(0, store.state.maximumMembers)) + " members, including you.")
				}
			}
		}
		Section {
			ForEach(store.state.people) { person in
				Button { store.send(.select(person)) } label: {
					HStack(spacing: 12) {
						Avatar(seed: person.id, label: DisplayInitials.person(person.label, isInboxID: person.handle == nil))
						IdentityText(inboxID: person.inboxID, handle: person.handle)
						Spacer()
						Image(systemName: store.state.recipients
							.contains(where: { $0.id == person.id }) ? "checkmark.circle.fill" : "circle").foregroundStyle(Theme.tint)
					}.foregroundStyle(.primary)
				}.buttonStyle(.plain)
			}
		} header: {
			HStack { Text("People from your chats", tableName: "NewConversation"); Spacer(); Text(
				"Saved on this device",
				tableName: "NewConversation",
			).font(.caption) }
		}
	}

	@ViewBuilder
	private var review: some View {
		if store.state.group {
			VStack(spacing: 12) {
				Avatar(
					seed: store.state.name,
					label: DisplayInitials.group(store.state.name.isEmpty ? "Group" : store.state.name),
					size: 72,
				)
				Text("Make it your group", tableName: "NewConversation")
					.font(.subheadline).foregroundStyle(Theme.secondaryLabel)
			}.frame(maxWidth: .infinity).listRowBackground(Color.clear)
			Section {
				TextField(localized("Group name"), text: Binding(get: { store.state.name }, set: { store.send(.name($0)) }))
				TextField(
					localized("Description (optional)"),
					text: Binding(get: { store.state.description }, set: { store.send(.description($0)) }),
					axis: .vertical,
				)
			}
		}
		Section {
			if store.state.group {
				HStack(spacing: 12) { Avatar(seed: "own", label: "Y"); Text("You", tableName: "NewConversation") }
			}
			ForEach(store.state.recipients) { person in HStack(spacing: 12) { Avatar(
				seed: person.id,
				label: DisplayInitials.person(person.label, isInboxID: person.handle == nil),
			); IdentityText(inboxID: person.inboxID, handle: person.handle) } }
			Button(localized("Change people")) { store.send(.back) }.buttonStyle(.glass).disabled(store.state.busy)
		} header: { Text("Members · " + String(store.state.recipients.count + 1)) }
		if store.state.group {
			Section {
				Picker(
					localized("Group permissions"),
					selection: Binding(get: { store.state.adminOnly }, set: { store.send(.adminOnly($0)) }),
				) {
					VStack(alignment: .leading, spacing: 4) { Text("All members", tableName: "NewConversation"); Text(
						"Members can update group details and add people.",
						tableName: "NewConversation",
					).font(.caption).foregroundStyle(Theme.secondaryLabel) }.tag(false)
					VStack(alignment: .leading, spacing: 4) { Text("Admins only", tableName: "NewConversation"); Text(
						"Only admins can update group details and add people.",
						tableName: "NewConversation",
					).font(.caption).foregroundStyle(Theme.secondaryLabel) }.tag(true)
				}.pickerStyle(.inline).labelsHidden()
			} header: { Text("Group permissions", tableName: "NewConversation") } footer: { Text(
				"You can change this later in the group details.",
				tableName: "NewConversation",
			) }
		}
	}

	private func localized(_ key: String) -> String {
		String(localized: String.LocalizationValue(key), table: "NewConversation")
	}
}

private struct RecipientFlow: Layout {
	var spacing: CGFloat = 8
	func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
		arrange(proposal.width ?? 320, subviews: subviews).size
	}

	func placeSubviews(in bounds: CGRect, proposal _: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
		let result = arrange(bounds.width, subviews: subviews)
		for (index, point) in result.points.enumerated() {
			subviews[index].place(at: CGPoint(x: bounds.minX + point.x, y: bounds.minY + point.y), proposal: .unspecified)
		}
	}

	private func arrange(_ width: CGFloat, subviews: Subviews) -> (size: CGSize, points: [CGPoint]) {
		var x: CGFloat = 0; var y: CGFloat = 0; var row: CGFloat = 0; var points: [CGPoint] = []
		for item in subviews {
			let size = item.sizeThatFits(.unspecified)
			if x > 0, x + size.width > width {
				x = 0; y += row + spacing; row = 0
			}
			points.append(CGPoint(x: x, y: y)); x += size.width + spacing; row = max(row, size.height)
		}
		return (CGSize(width: width, height: y + row), points)
	}
}
