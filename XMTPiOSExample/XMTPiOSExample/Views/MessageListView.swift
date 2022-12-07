//
//  MessageListView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/5/22.
//

import SwiftUI
import XMTP

struct MessageCellView: View {
	var myAddress: String
	var message: DecodedMessage

	var body: some View {
		HStack {
			if message.senderAddress == myAddress {
				Spacer()
			}
			Text(message.body)
				.padding(.vertical, 8)
				.padding(.horizontal, 12)
				.background(background)
				.cornerRadius(16)
				.foregroundColor(color)
			if message.senderAddress != myAddress {
				Spacer()
			}
		}
		.listRowSeparator(.hidden)
	}

	var background: Color {
		if message.senderAddress == myAddress {
			return .purple
		} else {
			return .secondary.opacity(0.2)
		}
	}

	var color: Color {
		if message.senderAddress == myAddress {
			return .white
		} else {
			return .primary
		}
	}
}

struct MessageListView: View {
	var myAddress: String
	var messages: [DecodedMessage]

	var body: some View {
		List {
			ForEach(Array(messages.sorted(by: { $0.sent < $1.sent }).enumerated()), id: \.0) { _, message in
				MessageCellView(myAddress: myAddress, message: message)
			}
		}
		.listStyle(.plain)
	}
}

struct MessageListView_Previews: PreviewProvider {
	static var previews: some View {
		MessageListView(
			myAddress: "0x00", messages: [
				XMTP.DecodedMessage(body: "Hello", senderAddress: "0x00", sent: Date().addingTimeInterval(-10)),
				XMTP.DecodedMessage(body: "Oh hi", senderAddress: "0x01", sent: Date().addingTimeInterval(-9)),
				XMTP.DecodedMessage(body: "Sup", senderAddress: "0x01", sent: Date().addingTimeInterval(-8)),
				XMTP.DecodedMessage(body: "Nice to see you", senderAddress: "0x00", sent: Date().addingTimeInterval(-7)),
				XMTP.DecodedMessage(body: "What if it's a longer message I mean really really long like should it wrap?", senderAddress: "0x01", sent: Date().addingTimeInterval(-6)),
				XMTP.DecodedMessage(body: "🧐", senderAddress: "0x00", sent: Date().addingTimeInterval(-5)),
			]
		)
	}
}
