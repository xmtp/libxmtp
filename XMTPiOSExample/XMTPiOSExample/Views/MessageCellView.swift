//
//  MessageCellView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/7/22.
//

import SwiftUI
import XMTPiOS

struct MessageTextView: View {
	var myAddress: String
	var message: Message
	var isGroup: Bool = false
	@State private var isDebugging = false

	var body: some View {
		VStack {
			HStack {
				if message.senderInboxId.lowercased() == myAddress.lowercased() {
					Spacer()
				}
				VStack(alignment: .leading) {
					if isGroup && message.senderInboxId.lowercased() != myAddress.lowercased() {
						Text(message.senderInboxId)
							.font(.caption)
							.foregroundStyle(.secondary)
					}

					Text(bodyText)

					if isDebugging {
						Text("My Address \(myAddress)")
							.font(.caption)
						Text("Sender Address \(message.senderInboxId)")
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
				if message.senderInboxId.lowercased() != myAddress.lowercased() {
					Spacer()
				}
			}
		}
	}

	var bodyText: String {
		do {
			return try message.content()
		} catch {
			do {
				return try message.fallbackContent
			} catch {
				return "Failed to retrieve content"
			}
		}
	}

	var background: Color {
		if message.senderInboxId.lowercased() == myAddress.lowercased() {
			return .purple
		} else {
			return .secondary.opacity(0.2)
		}
	}

	var color: Color {
		if message.senderInboxId.lowercased() == myAddress.lowercased() {
			return .white
		} else {
			return .primary
		}
	}
}

struct MessageGroupMembershipChangedView: View {
	var message: Message

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
	var message: Message
	var isGroup: Bool = false
	@State private var isDebugging = false

	var body: some View {
		do {
			switch try message.encodedContent.type {
			case ContentTypeText:
				return AnyView(MessageTextView(myAddress: myAddress, message: message))
			case ContentTypeGroupUpdated:
				return AnyView(MessageGroupMembershipChangedView(message: message))
			default:
				return AnyView(Text(try message.fallbackContent))
			}
		} catch {
			return AnyView(Text("Failed to load content"))
		}
	}
}
