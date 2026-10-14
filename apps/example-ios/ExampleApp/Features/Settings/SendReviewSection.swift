import ExampleCore
import SwiftUI

struct SendReviewSection: View {
	let graph: AppGraph
	@State private var drafts: [SendDraft] = []
	@State private var error: String?
	@State private var retrying: Set<UUID> = []
	var body: some View {
		Section("Send needs review") {
			if drafts.isEmpty {
				Text("No sends need review", tableName: "SendReview").foregroundStyle(.secondary)
			}
			ForEach(drafts) { draft in
				VStack(alignment: .leading, spacing: 8) {
					Text(String(localized: String.LocalizationValue(draft.deletionTargetID == nil ? "Send outcome unknown" :
							(draft.phase == .accepted ? "Deletion pending" : "Deletion outcome unknown")), table: "SendReview"))
						.font(.headline)
					Text(String(localized: String.LocalizationValue(draft.deletionTargetID == nil ?
					            	"The original message may already exist." : "Check this chat before trying to delete again."),
						table: "SendReview"))
						.font(.caption).foregroundStyle(.secondary)
					Text(draft.created, format: .dateTime.month().day().hour().minute()).font(.caption)
					Button(String(localized: "View chat", table: "SendReview")) { graph.navigate(.chat(draft.conversationID)) }
						.buttonStyle(.glass)
					if draft.deletionTargetID != nil, draft.phase == .accepted, draft.messageID != nil {
						Button(String(localized: "Retry deletion", table: "SendReview")) { retry(draft) }.buttonStyle(.glass)
							.disabled(retrying.contains(draft.id))
					}
					Button(String(localized: String.LocalizationValue(draft.deletionTargetID == nil ?
							"Clear saved send status" : "Clear saved deletion status"), table: "SendReview")) { clear(draft) }
						.buttonStyle(.glass).disabled(retrying.contains(draft.id))
				}
			}
			if !drafts.isEmpty {
				Text(
					String(localized: "sendReview.explanation", table: "SendReview"),
				)
				.font(.caption).foregroundStyle(.secondary)
			}
			if drafts.contains(where: { $0.deletionTargetID != nil }) {
				Text("Clearing this status does not undo the deletion.", tableName: "SendReview")
					.font(.caption).foregroundStyle(.secondary)
			}
			if let error {
				Text(String(localized: String.LocalizationValue(error), table: "SendReview")).foregroundStyle(.red)
			}
		}.task { reload() }
	}

	private func reload() {
		do {
			drafts = try ChatServices.get(graph).sends.all()
				.filter { $0.deletionTargetID != nil || ($0.phase == .queueing && $0.messageID == nil) }
		} catch {
			self.error = "Cannot load saved send status."
		}
	}

	private func retry(_ draft: SendDraft) {
		guard let messageID = draft.messageID, retrying.insert(draft.id).inserted else { return }
		do {
			let services = try ChatServices.get(graph)
			let origin = DestinationOrigin(session: services.key, token: graph.routes.token)
			graph.session.scope.add {
				do {
					try await services.coordinator.retry(conversationID: draft.conversationID, messageID: messageID,
					                                     admission: { graph.routes.admit(origin, fence: graph.fence) })
					guard graph.routes.admit(origin, fence: graph.fence) else { return }
					error = nil
				} catch {
					guard graph.routes.admit(origin, fence: graph.fence) else { return }
					self.error = "Deletion is still pending."
				}
				retrying.remove(draft.id)
				reload()
			}
		} catch {
			retrying.remove(draft.id)
			self.error = "Cannot load saved deletion status."
		}
	}

	private func clear(_ draft: SendDraft) {
		do {
			let services = try ChatServices.get(graph)
			try services.sends.remove(draft.id)
			if draft.deletionTargetID == nil {
				var composer = try services
					.composers.load(draft.conversationID)
				composer.release()
				try services.composers.save(
					composer,
					for: draft.conversationID,
				)
			}
			reload()
		} catch { self.error = "Cannot clear saved send status." }
	}
}
