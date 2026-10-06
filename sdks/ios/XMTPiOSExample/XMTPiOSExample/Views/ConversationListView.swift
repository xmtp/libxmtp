import SwiftUI
import XmtpSdk

struct ConversationListView: View {
	var client: SDKClient
	@EnvironmentObject var coordinator: EnvironmentCoordinator
	@State private var conversations: [Conversation] = []
	@State private var names: [String: String] = [:]
	@State private var isShowingNewConversation = false
	@State private var error: String?

	var body: some View {
		List {
			if let error {
				Text(error).foregroundStyle(.red)
			}
			ForEach(Array(conversations.sorted { $0.createdAt().ns > $1.createdAt().ns }.enumerated()),
			        id: \.offset)
			{ _, item in
				NavigationLink {
					switch item {
					case .dm: ConversationDetailView(client: client, conversation: item)
					case let .group(group): GroupDetailView(client: client, group: group)
					}
				} label: {
					VStack(alignment: .leading) {
						Text(names[item.id()] ?? item.id())
						Text(item.createdAt().date.formatted()).font(.caption)
					}
				}
			}
		}.navigationTitle("Conversations")
			.refreshable { await loadConversations() }
			.task { await loadConversations() }
			.task {
				do {
					for try await _ in try await client.conversations.stream() {
						await loadConversations()
					}
				} catch { self.error = error.localizedDescription }
			}
			.toolbar { Button("New conversation") { isShowingNewConversation = true } }
			.sheet(isPresented: $isShowingNewConversation) {
				NewConversationView(client: client) { _ in Task { await loadConversations() } }
			}
	}

	private func loadConversations() async {
		do {
			try await client.conversations.sync()
			conversations = try await client.conversations.list()
			for case let .group(group) in conversations {
				names[group.id()] = try await group.state().name
			}
		} catch { self.error = error.localizedDescription }
	}
}
