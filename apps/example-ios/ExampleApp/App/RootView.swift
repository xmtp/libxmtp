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
				graph.routes.dismissSheet()
			}
		})) {
			NavigationStack {
				if let route = graph.routes.sheet {
					destination(route)
						.toolbar {
							ToolbarItem(placement: .cancellationAction) {
								Button(String(localized: "common.close", table: "Common")) { graph.routes.back() }.buttonStyle(.glass)
							}
						}
				}
			}
		}
		.tint(Theme.tint).messengerMotion()
	}

	@ViewBuilder private func destination(_ route: Route) -> some View {
		if route == .connect {
			VStack(spacing: 24) {
				Image("Logo").resizable().scaledToFit().frame(width: 72, height: 72); Text("Join a server", tableName: "Foundation")
					.font(.largeTitle.bold()); Text("Your server. Your conversations.", tableName: "Foundation")
					.foregroundStyle(.secondary)
			}
		} else {
			ContentUnavailableView(
				title(route),
				systemImage: "bubble.left.and.bubble.right",
				description: Text("This screen is added in the next implementation stage.", tableName: "Foundation"),
			).navigationTitle(title(route))
		}
	}

	private func title(_ route: Route) -> String {
		switch route {
		case .connect: String(localized: "Join a server", table: "Foundation")
		case .chooseHandle: String(localized: "Choose a handle", table: "Foundation")
		case .chats: String(localized: "Chats", table: "Foundation")
		case .requests: String(localized: "Message requests", table: "Foundation")
		case .newConversation: String(localized: "New message", table: "Foundation")
		case .settings: String(localized: "Settings", table: "Foundation")
		case .changeHandle: String(localized: "Change handle", table: "Foundation")
		default: detailTitle(route)
		}
	}

	private func detailTitle(_ route: Route) -> String {
		switch route {
		case .chat: String(localized: "Conversation", table: "Foundation")
		case .details: String(localized: "Details", table: "Foundation")
		case .member: String(localized: "Member", table: "Foundation")
		case .permissions: String(localized: "Permissions", table: "Foundation")
		case .disappearing: String(localized: "Disappearing messages", table: "Foundation")
		case .sharedMedia: String(localized: "Media and files", table: "Foundation")
		case .groupFields: String(localized: "Group fields", table: "Foundation")
		case .myFields: String(localized: "My fields", table: "Foundation")
		default: actionTitle(route)
		}
	}

	private func actionTitle(_ route: Route) -> String {
		switch route {
		case .reactions: String(localized: "Reactions", table: "Foundation")
		case .emojiPicker: String(localized: "Choose a reaction", table: "Foundation")
		case .attachmentRecovery: String(localized: "Attachment recovery", table: "Foundation")
		default: String(localized: "XMTP Messenger", table: "Foundation")
		}
	}
}
