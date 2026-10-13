import ExampleCore
import SwiftUI

struct SendReviewSection: View {
	let graph: AppGraph
	@State private var drafts: [SendDraft] = []
	@State private var error: String?
	var body: some View {
		Section("Send needs review") {
			if drafts.isEmpty {
				Text("No sends need review", tableName: "SendReview").foregroundStyle(.secondary)
			}
			ForEach(drafts) { draft in
				VStack(alignment: .leading, spacing: 8) {
					Text("Send outcome unknown", tableName: "SendReview").font(.headline)
					Text("The original message may already exist.", tableName: "SendReview").font(.caption).foregroundStyle(.secondary)
					Text(draft.created, format: .dateTime.month().day().hour().minute()).font(.caption)
					Button(String(localized: "View chat", table: "SendReview")) { graph.navigate(.chat(draft.conversationID)) }
						.buttonStyle(.glass)
					Button(String(localized: "Clear saved send status", table: "SendReview")) { clear(draft) }.buttonStyle(.glass)
				}
			}
			if !drafts.isEmpty {
				Text(
					String(localized: "sendReview.explanation", table: "SendReview"),
				)
				.font(.caption).foregroundStyle(.secondary)
			}
			if let error {
				Text(String(localized: String.LocalizationValue(error), table: "SendReview")).foregroundStyle(.red)
			}
		}.task { reload() }
	}

	private func reload() {
		do { drafts = try ChatServices.get(graph).sends.all().filter { $0.phase == .queueing && $0.messageID == nil }
		} catch {
			self.error = "Cannot load saved send status."
		}
	}

	private func clear(_ draft: SendDraft) {
		do {
			let services = try ChatServices.get(graph)
			try services.sends.remove(draft.id)
			var composer = try services
				.composers.load(draft.conversationID)
			composer.release()
			try services.composers.save(
				composer,
				for: draft.conversationID,
			)
			reload()
		} catch { self.error = "Cannot clear saved send status." }
	}
}
