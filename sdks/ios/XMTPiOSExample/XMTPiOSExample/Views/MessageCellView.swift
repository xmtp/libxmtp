import SwiftUI
import XmtpSdk

struct MessageCellView: View {
	var myAddress: String
	var message: Message
	var isGroup = false

	private var isMine: Bool {
		message.senderInboxId == myAddress
	}

	private var text: String {
		switch message.content {
		case let .standard(.text(text)), let .standard(.markdown(text)): text
		case .standard(.groupUpdated): "Group membership changed"
		default: message.fallback ?? "Unsupported content"
		}
	}

	var body: some View {
		HStack {
			if isMine {
				Spacer()
			}
			VStack(alignment: .leading) {
				if isGroup, !isMine {
					Text(message.senderInboxId).font(.caption)
				}
				Text(text)
			}.padding(12)
				.background(isMine ? Color.purple : Color.secondary.opacity(0.2))
				.foregroundStyle(isMine ? Color.white : Color.primary)
				.cornerRadius(16)
			if !isMine {
				Spacer()
			}
		}
	}
}
