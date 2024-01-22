//
//  ConversationListView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import SwiftUI
import XMTPiOS

struct ConversationListView: View {
	var client: XMTPiOS.Client

	@EnvironmentObject var coordinator: EnvironmentCoordinator
	@State private var conversations: [XMTPiOS.Conversation] = []
	@State private var isShowingNewConversation = false

	var body: some View {
		List {
			ForEach(conversations, id: \.peerAddress) { conversation in
				NavigationLink(value: conversation) {
					Text(conversation.peerAddress)
				}
			}
		}
		.navigationDestination(for: Conversation.self) { conversation in
			ConversationDetailView(client: client, conversation: conversation)
		}
		.navigationTitle("Conversations")
		.refreshable {
			await loadConversations()
		}
		.task {
			await loadConversations()
		}
		.task {
			do {
				for try await conversation in await client.conversations.stream() {
					conversations.insert(conversation, at: 0)

					await add(conversations: [conversation])
				}

			} catch {
				print("Error streaming conversations: \(error)")
			}
		}
		.toolbar {
			ToolbarItem(placement: .navigationBarTrailing) {
				Button(action: {
					self.isShowingNewConversation = true
				}) {
					Label("New Conversation", systemImage: "plus")
				}
			}
		}
		.sheet(isPresented: $isShowingNewConversation) {
			NewConversationView(client: client) { conversation in
				conversations.insert(conversation, at: 0)
				coordinator.path.append(conversation)
			}
		}
	}

	func loadConversations() async {
		do {
			let conversations = try await client.conversations.list()

			await MainActor.run {
				self.conversations = conversations
			}

			await add(conversations: conversations)
		} catch {
			print("Error loading conversations: \(error)")
		}
	}

	func add(conversations: [Conversation]) async {
		// Ensure we're subscribed to push notifications on these conversations
		do {
			try await XMTPPush.shared.subscribe(topics: conversations.map(\.topic))
		} catch {
			print("Error subscribing: \(error)")
		}

		for conversation in conversations {
			do {
				try Persistence().save(conversation: conversation)
			} catch {
				print("Error saving \(conversation.topic): \(error)")
			}
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
