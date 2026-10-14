import ExampleCore
import SwiftUI

struct ReactionsFeatureView: View {
	@State private var cleanupKey = UUID().uuidString
	let graph: AppGraph
	let conversationID: String
	let messageID: String
	let emoji: String
	@State private var actions: MessageActionStore?
	@State private var selected = ""
	@State private var failure: String?
	var body: some View {
		VStack(spacing: 12) {
			HStack {
				Button(String(localized: "Close", table: "Chat"), systemImage: "xmark") { graph.routes.back() }
					.labelStyle(.iconOnly).buttonStyle(.glass).frame(width: 44, height: 44)
					.accessibilityIdentifier("reactions.close")
				Spacer()
				Text("Reactions", tableName: "Chat").font(.headline)
				Spacer()
				Color.clear.frame(width: 44, height: 44).accessibilityHidden(true)
			}.padding()

			if let row = actions?.state.message {
				let badges = MessageMapper.reactions(row.reactions, own: graph.active?.profile.inboxID ?? "")
				ScrollView(.horizontal) {
					HStack {
						Button(String(localized: "All", table: "Chat") + " " + String(badges.reduce(0) { $0 + $1.count })) {
							selected = ""
						}
						.buttonStyle(.glass); ForEach(badges) { badge in
							Button(badge.emoji + " " + String(badge.count)) { selected = badge.emoji }.buttonStyle(.glass)
								.accessibilityAddTraits(selected == badge.emoji ? .isSelected : [])
						}
					}
					.padding(.horizontal)
				}
				List { ForEach(badges.filter { selected.isEmpty || $0.emoji == selected }) { badge in ForEach(
					Array(badge.senders.enumerated()),
					id: \.offset,
				) { _, sender in HStack { Avatar(
					seed: sender,
					label: graph.identityDirectory?.identities[sender]?.handle
						.map { DisplayInitials.person($0) } ?? DisplayInitials.person(sender, isInboxID: true),
					size: 36,
				); IdentityText(
					inboxID: sender,
					handle: graph.identityDirectory?.identities[sender]?.handle,
				); Spacer(); Text(badge.emoji) } } } }
				if let badge = badges.first(where: { $0.mine && (selected.isEmpty || $0.emoji == selected) }),
				   actions?.allowed == true
				{
					Button(role: .destructive) { actions?.send(.react(row, badge.emoji)) } label: {
						Text("Remove my reaction", tableName: "Chat").frame(maxWidth: .infinity, minHeight: 44)
					}.buttonStyle(.glass).tint(.red).padding(.horizontal, 16).padding(.bottom, 16)
				}
			} else if let failure = actions?.state.error ?? failure {
				Text(String(localized: String.LocalizationValue(failure), table: "Chat"))
			} else {
				ProgressView()
			}
		}.presentationDetents([.medium, .large]).task(id: graph.routes.token) {
			let token = graph.routes.token
			await graph.session.scope.runCleanup(for: cleanupKey); actions = nil
			guard !Task.isCancelled, graph.routes.token == token, graph.routes.current == .reactions(
				conversationID: conversationID,
				messageID: messageID,
				emoji: emoji,
			) else { return }
			PreviewImages.shared.clear()
			selected = emoji
			do { let services = try ChatServices.get(graph); let origin = DestinationOrigin(
				session: services.key,
				token: graph.routes.token,
			); let value = MessageActionStore(
				queries: services.queries,
				coordinator: services.coordinator,
				origin: origin,
				admit: { graph.routes.admit($0, fence: graph.fence) },
				own: graph.active?.profile.inboxID ?? "",
				changed: {},
			); actions = value; cleanupKey = UUID().uuidString; graph.session.scope
				.setCleanup(for: cleanupKey) { await value.close() }; value.send(.load(messageID))
			} catch {
				failure = "Cannot load reactions. Try again."
			}
		}.onDisappear { let key = cleanupKey; Task { await graph.session.scope.runCleanup(for: key) } }
	}
}
