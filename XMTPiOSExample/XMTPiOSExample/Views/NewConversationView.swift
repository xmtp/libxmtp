//
//  NewConversationView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/7/22.
//

import SwiftUI
import XMTPiOS

struct NewConversationView: View {
	var client: XMTP.Client
	var onCreate: (Conversation) -> Void

	@Environment(\.dismiss) var dismiss
	@State private var recipientAddress: String = ""
	@State private var error: String?

	var body: some View {
		Form {
			Section("Recipient Address") {
				TextField("Enter address here…", text: $recipientAddress)
					.onChange(of: recipientAddress) { newAddress in
						check(address: newAddress)
					}

				if let error {
					Text(error)
						.font(.caption)
						.foregroundColor(.secondary)
				}
			}
		}
		.presentationDetents([.height(100), .height(120)])
		.navigationTitle("New conversation")
	}

	private func check(address: String) {
		if address.count != 42 {
			return
		}

		error = nil

		Task {
			do {
				let conversation = try await client.conversations.newConversation(with: address)
				await MainActor.run {
					dismiss()
					onCreate(conversation)
				}
			} catch ConversationError.recipientNotOnNetwork {
				await MainActor.run {
					self.error = "Recipient is not on the XMTP network."
				}
			} catch {
				await MainActor.run {
					self.error = error.localizedDescription
				}
			}
		}
	}
}

struct NewConversationView_Previews: PreviewProvider {
	static var previews: some View {
		NavigationStack {
			VStack {
				PreviewClientProvider { client in
					Text("Hi")
						.sheet(isPresented: .constant(true)) {
							NewConversationView(client: client) { _ in }
						}
				}
			}
		}
	}
}
