//
//  MessageCellView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/7/22.
//

import SwiftUI
import XMTPiOS
import web3

struct MessageTextView: View {
	var myAddress: String
	var message: DecodedMessage
	var isGroup: Bool = false
	@State private var isDebugging = false

	var body: some View {
		VStack {
			HStack {
				if message.senderAddress.lowercased() == myAddress.lowercased() {
					Spacer()
				}
				VStack(alignment: .leading) {
					if isGroup && message.senderAddress.lowercased() != myAddress.lowercased() {
						Text(message.senderAddress)
							.font(.caption)
							.foregroundStyle(.secondary)
					}

					Text(bodyText)

					if isDebugging {
						Text("My Address \(myAddress)")
							.font(.caption)
						Text("Sender Address \(message.senderAddress)")
							.font(.caption)
					}
				}
				.padding(.vertical, 8)
				.padding(.horizontal, 12)
				.background(background)
				.cornerRadius(16)
				.foregroundColor(color)
				.onTapGesture {
					withAnimation {
						isDebugging.toggle()
					}
				}
				if message.senderAddress.lowercased() != myAddress.lowercased() {
					Spacer()
				}
			}
		}
	}

	var bodyText: String {
		do {
			return try message.content()
		} catch {
			return message.fallbackContent
		}
	}

	var background: Color {
		if message.senderAddress.lowercased() == myAddress.lowercased() {
			return .purple
		} else {
			return .secondary.opacity(0.2)
		}
	}

	var color: Color {
		if message.senderAddress.lowercased() == myAddress.lowercased() {
			return .white
		} else {
			return .primary
		}
	}
}

struct MessageGroupMembershipChangedView: View {
	var message: DecodedMessage

	var body: some View {
		Text(label)
			.font(.caption)
			.foregroundStyle(.secondary)
			.padding(.vertical)
	}

	var label: String {
		do {
			let changes: GroupUpdated = try message.content()

			if !changes.addedInboxes.isEmpty {
				return "Added \(changes.addedInboxes.map(\.inboxID).map { Util.abbreviate(address: $0) }.joined(separator: ", "))"
			}

			if !changes.removedInboxes.isEmpty {
				return "Removed \(changes.removedInboxes.map(\.inboxID).map { Util.abbreviate(address: $0) }.joined(separator: ", "))"
			}

			return changes.debugDescription
		} catch {
			return "Membership changed"
		}

	}
}

struct MessageCellView: View {
	var myAddress: String
	var message: DecodedMessage
	var isGroup: Bool = false
	@State private var isDebugging = false

	var body: some View {
		switch message.encodedContent.type {
		case ContentTypeText:
			MessageTextView(myAddress: myAddress, message: message)
		case ContentTypeGroupUpdated:
			MessageGroupMembershipChangedView(message: message)
		default:
			Text(message.fallbackContent)
		}
	}
}

struct MessageCellView_Previews: PreviewProvider {
	static var previews: some View {
		PreviewClientProvider { client in
			List {
				MessageCellView(myAddress: "0x00", message: DecodedMessage.preview(client: client, topic: "foo", body: "Hi, how is it going?", senderAddress: "0x00", sent: Date()))
			}
			.listStyle(.plain)
		}
	}
}
