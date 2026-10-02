import SwiftUI
import XmtpSdk

/// Display the conversation.
struct ConversationView: View {
	@Environment(XmtpSession.self) private var session
	let conversationId: String
	var body: some View {
		VStack(spacing: 0) {
			let messages = (session.conversationMessages[conversationId].value ?? [Message]()).reversed()
			ScrollViewReader { proxy in
				List(messages, id: \.id) { message in
					MessageView(conversationId: conversationId, message: message)
						.id(message.id)
				}
				.listRowSpacing(16)
				.onChange(of: messages.last?.id) { _, lastId in
					proxy.scrollTo(lastId ?? "")
				}
			}
			MessageComposerView(conversationId: conversationId)
		}
		.refreshable {
			Task {
				try await session.refreshConversation(conversationId: conversationId)
			}
		}
		.navigationTitle(session.conversationNames[conversationId].value ?? "Conversation")
	}
}

struct MessageView: View {
	let conversationId: String
	let message: Message
	@Environment(XmtpSession.self) private var session
	var body: some View {
		let isMe = message.senderInboxId == session.inboxId
		VStack(alignment: isMe ? .trailing : .leading) {
			HStack {
				if isMe {
					Spacer()
				}
				InboxNameText(inboxId: message.senderInboxId)
					.foregroundColor(.secondary)
					.font(.caption2)
				if !isMe {
					Spacer()
				}
			}
			Text(message.displayText)
				.foregroundColor(.primary)
				.font(.body)
				.padding(.vertical)
			Spacer()
			HStack {
				Spacer()
				Text(message.sentAt.date.formatted())
					.foregroundColor(.secondary)
					.font(.caption2)
			}
		}
		.padding(.top, 6)
		.padding(.bottom, 4)
	}
}

struct MessageComposerView: View {
	@Environment(XmtpSession.self) private var session
	@State private var message = ""
	@State private var isSending = false
	@State private var error: String?
	@FocusState var isFocused
	let conversationId: String
	var body: some View {
		VStack {
			TextField("Message", text: $message)
				.focused($isFocused)
				.disabled(isSending)
				.padding(4)
				.onSubmit {
					Task {
						isSending = true
						defer { isSending = false }
						do {
							if try await session.sendMessage(message, to: conversationId) {
								message = ""
								error = nil
							}
						} catch { self.error = error.localizedDescription }
					}
				}
				.textInputAutocapitalization(.never)
				.disableAutocorrection(true)
				.textFieldStyle(.roundedBorder)
				.onAppear { isFocused = true }
				.submitLabel(.send)
			if let error {
				Text(error).foregroundStyle(.red)
			}
		}
		.padding(4)
	}
}

private extension Message {
	var displayText: String {
		switch content {
		case let .standard(.text(text)), let .standard(.markdown(text)): text
		case .standard(.groupUpdated): "Group membership changed"
		default: fallback ?? "Unsupported content"
		}
	}
}
