import SwiftUI
import XMTPiOS

struct ConversationListView: View {
	var client: XMTPiOS.Client

	@EnvironmentObject var coordinator: EnvironmentCoordinator
	@State private var conversations: [XMTPiOS.Conversation] = []
	@State private var isShowingNewConversation = false

	/// Pre-sorted conversations to reduce complexity
	private var sortedConversations: [XMTPiOS.Conversation] {
		conversations.sorted(by: compareConversations)
	}

	var body: some View {
		List {
			ForEach(sortedConversations, id: \.id) { item in
				NavigationLink(destination: destinationView(for: item)) {
					conversationRow(for: item)
				}
			}
		}
		.navigationTitle("Conversations")
		.refreshable { await loadConversations() }
		.task { await loadConversations() }
		.task { await startConversationStream() }
		.toolbar {
			ToolbarItem(placement: .navigationBarTrailing) {
				Button(action: { isShowingNewConversation = true }) {
					Label("New Conversation", systemImage: "plus")
				}
			}
		}
		.sheet(isPresented: $isShowingNewConversation) {
			NewConversationView(client: client) { conversationOrGroup in
				addConversation(conversationOrGroup)
			}
		}
	}

	/// Helper function to compare conversations by createdAt date
	private func compareConversations(_ lhs: XMTPiOS.Conversation, _ rhs: XMTPiOS.Conversation) -> Bool {
		lhs.createdAt > rhs.createdAt
	}

	/// Extracted row view for each conversation
	private func conversationRow(for item: XMTPiOS.Conversation) -> some View {
		HStack {
			conversationIcon(for: item)
			VStack(alignment: .leading) {
				Text(conversationDisplayName(for: item))
					.foregroundStyle(.secondary)
				Text(formattedDate(for: item.createdAt))
					.font(.caption)
					.foregroundStyle(.secondary)
			}
		}
	}

	/// Extracted icon view for conversation type
	@ViewBuilder
	private func conversationIcon(for item: XMTPiOS.Conversation) -> some View {
		switch item {
		case .dm:
			Image(systemName: "person.fill")
				.resizable()
				.scaledToFit()
				.frame(width: 16, height: 16)
				.foregroundStyle(.secondary)
		case .group:
			Image(systemName: "person.3.fill")
				.resizable()
				.scaledToFit()
				.frame(width: 16, height: 16)
				.foregroundStyle(.secondary)
		}
	}

	/// Helper function to provide a display name based on the conversation type
	private func conversationDisplayName(for item: XMTPiOS.Conversation) -> String {
		switch item {
		case let .dm(conversation):
			return (try? Util.abbreviate(address: conversation.peerInboxId)) ?? "Unknown Address"
		case let .group(group):
			let name = (try? group.name()) ?? ""
			return name.isEmpty ? "Group Id: \(group.id)" : name
		}
	}

	/// Helper function to format the date
	private func formattedDate(for date: Date) -> String {
		date.formatted()
	}

	/// Define destination view based on conversation type
	@ViewBuilder
	private func destinationView(for item: XMTPiOS.Conversation) -> some View {
		switch item {
		case let .dm(conversation):
			ConversationDetailView(client: client, conversation: .dm(conversation))
		case let .group(group):
			GroupDetailView(client: client, group: group)
		}
	}

	/// Async function to load conversations
	func loadConversations() async {
		do {
			try await client.conversations.sync()
			let loadedConversations = try await client.conversations.list()
			await MainActor.run {
				conversations = loadedConversations
			}
			await add(conversations: loadedConversations)
		} catch {
			print("Error loading conversations: \(error)")
		}
	}

	/// Async function to stream conversations
	func startConversationStream() async {
		do {
			for try await conversation in try await client.conversations.stream() {
				await MainActor.run {
					conversations.insert(conversation, at: 0)
				}
				await add(conversations: [conversation])
			}
		} catch {
			print("Error streaming conversations: \(error)")
		}
	}

	/// Helper function to add a conversation or group
	private func addConversation(_ conversationOrGroup: XMTPiOS.Conversation) {
		switch conversationOrGroup {
		case let .dm(conversation):
			conversations.insert(.dm(conversation), at: 0)
			coordinator.path.append(conversationOrGroup)
		case let .group(group):
			conversations.insert(.group(group), at: 0)
			coordinator.path.append(conversationOrGroup)
		}
	}

	func add(conversations: [XMTPiOS.Conversation]) async {
		for conversationOrGroup in conversations {
			switch conversationOrGroup {
			case .dm, .group:
				return
			}
		}
	}
}

struct ConversationListView_Previews: PreviewProvider {
	static var previews: some View {
		PreviewClientProvider { client in
			NavigationStack {
				ConversationListView(client: client)
			}
		}
	}
}
