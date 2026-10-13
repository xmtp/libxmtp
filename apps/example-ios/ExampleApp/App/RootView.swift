import ExampleCore
import SwiftUI

struct RootView: View {
	@Bindable var graph: AppGraph
	var body: some View {
		NavigationStack(path: Binding(get: { graph.routes.path }, set: { graph.routes.setPath($0) })) {
			destination(graph.routes.root)
				.navigationDestination(for: Route.self) { destination($0) }
		}
		.sheet(isPresented: Binding(get: { graph.routes.sheet != nil }, set: {
			if !$0 {
				graph.routes.back()
			}
		})) {
			NavigationStack {
				if let route = graph.routes.sheet {
					destination(route)
						.toolbar {
							ToolbarItem(placement: .cancellationAction) { Button("Close") { graph.routes.back() }.buttonStyle(.glass) }
						}
				}
			}
		}
		.tint(Theme.tint).messengerMotion()
	}

	@ViewBuilder private func destination(_ route: Route) -> some View {
		if route == .connect {
			VStack(spacing: 24) {
				Image("Logo").resizable().scaledToFit().frame(width: 72, height: 72); Text("Join a server")
					.font(.largeTitle.bold()); Text("Your server. Your conversations.").foregroundStyle(.secondary)
			}
		} else {
			ContentUnavailableView(
				title(route),
				systemImage: "bubble.left.and.bubble.right",
				description: Text("This screen is added in the next implementation stage."),
			).navigationTitle(title(route))
		}
	}

	private func title(_ route: Route) -> String {
		switch route {
		case .connect: "Join a server"
		case .chooseHandle: "Choose a handle"
		case .chats: "Chats"
		case .requests: "Message requests"
		case .newConversation: "New message"
		case .chat: "Conversation"
		case .details: "Details"
		case .member: "Member"
		case .permissions: "Permissions"
		case .disappearing: "Disappearing messages"
		case .sharedMedia: "Media and files"
		case .groupFields: "Group fields"
		case .myFields: "My fields"
		case .reactions: "Reactions"
		case .emojiPicker: "Choose a reaction"
		case .settings: "Settings"
		case .changeHandle: "Change handle"
		case .attachmentRecovery: "Attachment recovery"
		}
	}
}
