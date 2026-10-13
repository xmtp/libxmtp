import ExampleCore
import SwiftUI

struct AttachmentRecoveryFeatureView: View {
	@State private var cleanupKey = UUID().uuidString
	let graph: AppGraph
	@State private var store: AttachmentStore?
	@State private var failure: String?
	var body: some View {
		List {
			Section {
				Text(
					"Upload drafts are retained for 24 hours by default. Review an interrupted send before you send another file.",
					tableName: "AttachmentRecovery",
				)
				.foregroundStyle(.secondary)
			} header: { Text("Files that need review", tableName: "AttachmentRecovery") }
			if let store {
				ForEach(store.state.drafts) { draft in
					Section {
						Label(draft.card.name, systemImage: "doc")
						Text(draft
							.phase == .unknown ?
							"Review send outcome. The original message may already be in the chat. Review it before you send another file." :
							draft
							.conversationID == nil ? "Choose a chat for this draft. Unassigned file. Discard it or select a chat." : draft
							.phase.rawValue.capitalized).font(.caption).foregroundStyle(.secondary)
						if let id = draft.conversationID {
							Button(String(localized: "View chat", table: "AttachmentRecovery")) { graph.navigate(.chat(id)) }
								.buttonStyle(.glass)
						} else {
							ForEach(graph.conversations?.state.rows ?? []) { row in
								Button("Assign to " + row.title) { store.send(.assign(
									draft.id,
									row.id,
								)) }.buttonStyle(.glass)
							}
						}
						Button(String(localized: "Discard", table: "AttachmentRecovery"), role: .destructive) {
							store.send(.discard(draft.id))
						}.buttonStyle(.glass)
					}
				}
				if store.state.drafts.isEmpty {
					Text("No files need review", tableName: "AttachmentRecovery")
				}
				if let error = store.state.error {
					Text(String(localized: String.LocalizationValue(error), table: "AttachmentRecovery")).foregroundStyle(.red)
				}
			}
			if let failure {
				Text(String(localized: String.LocalizationValue(failure), table: "AttachmentRecovery"))
			}
		}.navigationTitle(String(localized: "Attachment recovery", table: "AttachmentRecovery"))
			.task(id: graph.routes.token) {
				let token = graph.routes.token
				await graph.session.scope.runCleanup(for: cleanupKey); store = nil
				guard !Task.isCancelled, graph.routes.token == token, graph.routes.current == .attachmentRecovery else { return }
				PreviewImages.shared.clear()
				do { let services = try ChatServices.get(graph); let origin = DestinationOrigin(
					session: services.key,
					token: graph.routes.token,
				); let value = AttachmentStore(
					gateway: services.gateway,
					staging: services.staging,
					drafts: services.files,
					coordinator: services.coordinator,
					conversationID: nil,
					origin: origin,
					admit: { graph.routes.admit($0, fence: graph.fence) },
					allowed: { false },
				); store = value; cleanupKey = UUID().uuidString; graph.session.scope
					.setCleanup(for: cleanupKey) { await value.close() }; value.send(.load)
				} catch {
					failure = "Cannot load saved files. Try again."
				}
			}.onDisappear { let key = cleanupKey; Task { await graph.session.scope.runCleanup(for: key) } }
	}
}
