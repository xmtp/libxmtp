//
//  MessageCellView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/7/22.
//

import SwiftUI
import XMTP

struct MessageCellView: View {
	var myAddress: String
	var message: DecodedMessage
	@State private var isDebugging = false

	var body: some View {
		VStack {
			HStack {
				if message.senderAddress == myAddress {
					Spacer()
				}
				VStack(alignment: .leading) {
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
				if message.senderAddress != myAddress {
					Spacer()
				}
			}
		}
	}

	var bodyText: String {
		// swiftlint:disable force_try
		return try! message.content()
		// swiftlint:enable force_try
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

struct MessageCellView_Previews: PreviewProvider {
	static var previews: some View {
		List {
			MessageCellView(myAddress: "0x00", message: DecodedMessage.preview(body: "Hi, how is it going?", senderAddress: "0x00", sent: Date()))
		}
		.listStyle(.plain)
	}
}
