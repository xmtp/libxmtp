import SwiftUI
import XMTPiOS

// Display the conversation.
struct ConversationView: View {
    @Environment(XmtpSession.self) private var session
    let conversationId: String
    var body: some View {
        VStack(spacing: 0) {
            let messages = (session.conversationMessages[conversationId].value ?? [DecodedMessage]()).reversed()
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
        .navigationTitle(session.conversations[conversationId].value?.name ?? "")
    }
}

struct MessageView: View {
    let conversationId: String
    let message: DecodedMessage
    @Environment(XmtpSession.self) private var session
    var body: some View {
        let isMe = message.senderInboxId == session.inboxId
        VStack(alignment: isMe ? .trailing : .leading) {
            HStack {
                if (isMe) {
                    Spacer()
                }
                InboxNameText(inboxId: message.senderInboxId)
                    .foregroundColor(.secondary)
                    .font(.caption2)
                if (!isMe) {
                    Spacer()
                }
            }
            Text((try? message.body) ?? "")
                .foregroundColor(.primary)
                .font(.body)
                .padding(.vertical)
            Spacer()
            HStack {
                Spacer()
                Text(message.sentAt.description)
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
    @State private var message: String = ""
    @State private var isSending = false
    @FocusState var isFocused
    let conversationId: String
    var body: some View {
        HStack {
            TextField("Message", text: $message)
                .focused($isFocused)
                .disabled(isSending)
                .padding(4)
                .onSubmit {
                    Task {
                        defer {
                            isSending = false
                        }
                        isSending = true
                        if (try await session.sendMessage(message, to: conversationId)) {
                            message = ""
                        }
                    }
                }
                .textInputAutocapitalization(.never)
                .disableAutocorrection(true)
                .textFieldStyle(.roundedBorder)
                .onAppear {
                    isFocused = true
                }
                .submitLabel(.send)
        }
        .padding(4)
    }
    
    
}
