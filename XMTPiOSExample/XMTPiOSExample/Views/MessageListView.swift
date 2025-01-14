//
//  MessageListView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/5/22.
//

import SwiftUI
import XMTPiOS

struct MessageListView: View {
	var myAddress: String
	var messages: [Message]
	var isGroup: Bool = false

	var body: some View {
		ScrollViewReader { proxy in
			ScrollView {
				if messages.isEmpty {
					Text("No messages yet.")
						.foregroundStyle(.secondary)
				}

				VStack {
					ForEach(Array(messages.sorted(by: { $0.sentAt < $1.sentAt }).enumerated()), id: \.0) { i, message in
						MessageCellView(myAddress: myAddress, message: message, isGroup: isGroup)
							.transition(.scale)
							.id(i)
					}
					Spacer()
						.onChange(of: messages.count) { _ in
							withAnimation {
								proxy.scrollTo(messages.count - 1, anchor: .bottom)
							}
						}
				}
			}
			.padding(.horizontal)
		}
	}
}
