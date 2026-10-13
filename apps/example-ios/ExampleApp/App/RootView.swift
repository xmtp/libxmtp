import ExampleCore
import SwiftUI

struct RootView: View {
	@Bindable var graph: AppGraph
	@Namespace private var composeTransition
	@Environment(\.accessibilityReduceMotion) private var reduceMotion
	var body: some View {
		Group {
			if graph.isRestoring {
				ProgressView()
			} else {
				NavigationStack(path: Binding(get: { graph.routes.path }, set: { graph.routes.setPath($0) })) {
					destination(graph.routes.root)
						.navigationDestination(for: Route.self) { destination($0) }
				}
			}
		}
		.sheet(isPresented: Binding(get: { graph.routes.sheet != nil }, set: {
			if !$0 {
				graph.routes.dismissSheet()
			}
		})) {
			NavigationStack {
				if let route = graph.routes.sheet {
					destination(route).toolbar {
						if route != .newConversation {
							ToolbarItem(placement: .cancellationAction) {
								Button("Close", systemImage: "xmark") { graph.routes.back() }.labelStyle(.iconOnly).buttonStyle(.glass)
							}
						}
					}
				}
			}
		}
		.tint(Theme.tint).messengerMotion()
		.preferredColorScheme(graph.appearance == "dark" ? .dark : graph.appearance == "light" ? .light : nil)
		.alert("Cannot open account", isPresented: Binding(get: { graph.error != nil }, set: {
			if !$0 {
				graph.error = nil
			}
		})) {
			Button("Retry") { Task { await graph.bindSession() } }
			Button("Cancel", role: .cancel) { graph.error = nil }
		} message: { Text(graph.error ?? "") }
		.task { await graph.restore() }
		.onChange(of: graph.routes.token) { _, _ in Task { await graph.closeInactiveDestinations() } }
		.onChange(of: graph.session.state) { _, _ in Task { await graph.bindSession() } }
	}

	@ViewBuilder private func destination(_ route: Route) -> some View {
		switch route {
		case .connect: ConnectView(store: graph.connect)
		case .chooseHandle, .changeHandle: handleDestination
		case .chats: chatsDestination
		case .requests: requestsDestination
		case .settings: SettingsView(graph: graph)
		case .newConversation: newMessageDestination
		default: detailsDestination(route)
		}
	}

	@ViewBuilder private var handleDestination: some View {
		if let handles = graph.handles {
			HandleView(store: handles)
		}
	}

	@ViewBuilder private var chatsDestination: some View {
		if let store = graph.conversations, let decisions = graph.requests {
			ChatsView(store: store, decisions: decisions, onNavigate: graph.navigate,
			          serverName: graph.serverName, composeNamespace: composeTransition)
		}
	}

	@ViewBuilder private var requestsDestination: some View {
		if let store = graph.requestConversations, let decisions = graph.requests {
			RequestsView(store: store, decisions: decisions, onNavigate: graph.navigate)
		}
	}

	@ViewBuilder private var newMessageDestination: some View {
		if let store = graph.newConversation {
			NewConversationView(store: store, onClose: { graph.routes.back() })
				.modifier(ComposeDestination(namespace: composeTransition, reduceMotion: reduceMotion))
		}
	}

	@ViewBuilder private func detailsDestination(_ route: Route) -> some View {
		switch route {
		case let .chat(id): ChatFeatureView(graph: graph, id: id)
		case let .details(id): DetailsFeatureView(graph: graph, id: id)
		case let .member(conversationID, inboxID): MemberFeatureView(
				graph: graph,
				conversationID: conversationID,
				inboxID: inboxID,
			)
		case let .permissions(id): PermissionsFeatureView(graph: graph, id: id)
		case let .disappearing(id): DisappearingFeatureView(graph: graph, id: id)
		case let .sharedMedia(id): SharedMediaFeatureView(graph: graph, id: id)
		case let .groupFields(id): GroupFieldsFeatureView(graph: graph, id: id)
		case let .myFields(id): MyFieldsFeatureView(graph: graph, id: id)
		default: messageDestination(route)
		}
	}

	@ViewBuilder private func messageDestination(_ route: Route) -> some View {
		switch route {
		case let .reactions(conversationID, messageID, emoji):
			ReactionsFeatureView(graph: graph, conversationID: conversationID, messageID: messageID, emoji: emoji)
		case let .emojiPicker(conversationID, messageID):
			EmojiPickerFeatureView(graph: graph, conversationID: conversationID, messageID: messageID)
		case .attachmentRecovery: AttachmentRecoveryFeatureView(graph: graph)
		default: EmptyView()
		}
	}
}

private struct ComposeDestination: ViewModifier {
	let namespace: Namespace.ID
	let reduceMotion: Bool
	func body(content: Content) -> some View {
		if reduceMotion {
			content.transition(.opacity)
		} else {
			content.navigationTransition(.zoom(sourceID: "new-message", in: namespace))
		}
	}
}
