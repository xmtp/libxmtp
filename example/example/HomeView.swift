import SwiftData
import SwiftUI
import XMTPiOS

// Screen displayed by default when the user has logged in.
//
// This has two tabs:
//  - "Chats" (listing conversations that can be explored)
//  - "Settings" (allowing you to log out etc)
// And it displays the button for creating new chats.
struct HomeView: View {
	@Environment(XmtpSession.self) private var session
	@Environment(Router.self) private var router
	var body: some View {
		@Bindable var router = router
		TabView {
			Group {
				NavigationStack(path: $router.routes) {
					ConversationList()
						// We can do this because `Route` implements `View`
						// This lets us link to a Route from NavigationLinks elsewhere.
						.navigationDestination(for: Route.self) { $0 }
						.toolbar {
							Button {
								router.push(route: .createConversation)
							} label: {
								Image(systemName: "plus")
							}
						}
				}
				.tabItem {
					Label("Chats", systemImage: "bubble.left.and.bubble.right")
				}
				SettingsView()
					.tabItem {
						Label("Settings", systemImage: "gearshape")
					}
			}
		}
	}
}

// List the conversations for the active session.
//
// This refreshes the list when it appears.
// It also supports pull-to-refresh.
private struct ConversationList: View {
	@Environment(XmtpSession.self) private var session
	var body: some View {
		List(session.conversationIds, id: \.self) { cId in
			NavigationLink(value: Route.conversation(conversationId: cId)) {
				ConversationItem(conversationId: cId)
			}
		}
		.onAppear {
			Task {
				try await session.refreshConversations()
			}
		}
		.refreshable {
			Task {
				try await session.refreshConversations()
			}
		}
	}
}

// Show an item in the conversation list.
private struct ConversationItem: View {
	@Environment(XmtpSession.self) private var session
	var conversationId: String
	var body: some View {
		VStack(alignment: .leading, spacing: 3) {
			// TODO: something prettier
			Text(session.conversations[conversationId].value?.name ?? "Untitled")
				.foregroundColor(.primary)
				.lineLimit(1)
				.font(.headline)
			Text("\(session.conversationMembers[conversationId].value?.count ?? 0) members")
				.foregroundColor(.secondary)
				.font(.subheadline)
		}
	}
}

// Allow the user to logout (and change TBD other settings)
private struct SettingsView: View {
	@Environment(XmtpSession.self) private var session
	var body: some View {
		VStack(alignment: .leading, spacing: 3) {
			// TODO: more complete settings view
			Text(session.inboxId ?? "")
				.foregroundColor(.primary)
				.lineLimit(1)
				.font(.headline)
			Button("Logout") {
				Task {
					try await session.clear()
				}
			}
		}.padding()
	}
}
