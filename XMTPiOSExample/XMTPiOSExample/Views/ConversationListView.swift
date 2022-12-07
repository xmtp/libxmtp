//
//  ConversationListView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import SwiftUI
import XMTP

struct ConversationListView: View {
	var client: XMTP.Client
	@State private var conversations: [XMTP.Conversation] = []

	var body: some View {
		List {
			ForEach(conversations, id: \.peerAddress) { conversation in
				NavigationLink(destination: ConversationDetailView(client: client, conversation: conversation)) {
					Text(conversation.peerAddress)
				}
			}
		}
		.navigationTitle("Conversations")
		.refreshable {
			await loadConversations()
		}
		.task {
			await loadConversations()
		}
	}

	func loadConversations() async {
		do {
			let conversations = try await client.conversations.list()

			await MainActor.run {
				self.conversations = conversations
			}
		} catch {
			print("Error loading conversations: \(error)")
		}
	}
}

struct ConversationListView_Previews: PreviewProvider {
	static var previews: some View {
		VStack {
			PreviewClientProvider { client in
				NavigationView {
					ConversationListView(client: client)
				}
			}
		}
	}
}
