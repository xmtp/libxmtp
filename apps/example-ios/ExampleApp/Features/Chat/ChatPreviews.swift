import ExampleCore
import SwiftUI

/// Core values keep the key transcript states reviewable without a server.
private enum ChatPreviewFixtures {
	static let messages: [ChatMessage] = [
		ChatMessage(
			id: "incoming",
			conversationID: "preview",
			sender: "maya",
			sentAt: 1,
			content: .text("Here is the config file from staging."),
		),
		ChatMessage(
			id: "reply",
			conversationID: "preview",
			sender: "you",
			sentAt: 2,
			content: .reply(quote: "Here is the config file from staging.", body: "Thanks! Checking the limits now."),
		),
		ChatMessage(
			id: "failed",
			conversationID: "preview",
			sender: "you",
			sentAt: 3,
			delivery: .failed,
			content: .text("Can you resend it with the new limits?"),
		),
		ChatMessage(id: "deleted", conversationID: "preview", sender: "maya", sentAt: 4, content: .deleted),
	]
}

private struct ChatPreview: View {
	var body: some View {
		NavigationStack {
			VStack(spacing: 18) {
				Text("Today 9:41 AM").font(.caption2).foregroundStyle(.secondary)
				ForEach(ChatPreviewFixtures.messages) { row in
					let mine = row.sender == "you"
					HStack {
						if mine {
							Spacer(minLength: 44)
						}
						VStack(alignment: mine ? .trailing : .leading, spacing: 5) {
							Bubble(isOutgoing: mine) {
								if case let .reply(quote, text) = row
									.content
								{
									VStack(alignment: .leading, spacing: 6) { Text(quote).font(.caption).padding(8).background(
										.black.opacity(0.08),
										in: RoundedRectangle(cornerRadius: 8),
									); Text(text) }
								} else {
									Text(row.content.text)
								}
							}
							if row
								.delivery == .failed
							{
								Text("Not sent · Tap to retry or edit").font(.caption2).foregroundStyle(.secondary)
							}
						}
						if !mine {
							Spacer(minLength: 44)
						}
					}
				}
				Spacer()
				HStack { Button("Add attachment", systemImage: "plus") {}.labelStyle(.iconOnly).buttonStyle(.glass); TextField(
					"Message",
					text: .constant(""),
				).padding(12).glassEffect(.regular, in: Capsule()) }
			}.padding().navigationTitle("@maya").navigationBarTitleDisplayMode(.inline)
		}
	}
}

#Preview("Chat · Light") { ChatPreview().preferredColorScheme(.light) }
#Preview("Chat · Dark") { ChatPreview().preferredColorScheme(.dark) }
#Preview("Chat · AX3") { ChatPreview().dynamicTypeSize(.accessibility3) }
