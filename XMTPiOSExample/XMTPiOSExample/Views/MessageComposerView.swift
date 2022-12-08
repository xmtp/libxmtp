//
//  MessageComposerView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import SwiftUI
import XMTP

struct MessageComposerView: View {
	@State private var text: String = ""
	@State private var isSending = false
	@FocusState var isFocused

	var onSend: (String) async -> Void

	var body: some View {
		HStack {
			TextField("Type something…", text: $text, onCommit: send)
				.textFieldStyle(.roundedBorder)
				.focused($isFocused)
				.onAppear {
					self.isFocused = true
				}
			Button(action: send) {
				Label("Send", systemImage: "arrow.up.circle.fill")
					.font(.title)
					.labelStyle(.iconOnly)
			}
			.tint(.purple)
		}
		.disabled(isSending)
		.padding(4)
	}

	func send() {
		if text.trimmingCharacters(in: .whitespacesAndNewlines) == "" {
			return
		}

		isSending = true
		Task {
			await onSend(text)
			await MainActor.run {
				self.text = ""
				self.isSending = false
				self.isFocused = true
			}
		}
	}
}

struct MessageComposerView_Previews: PreviewProvider {
	static var previews: some View {
		MessageComposerView { _ in }
	}
}
