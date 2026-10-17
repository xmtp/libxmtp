import ExampleCore
import SwiftUI
import UIKit

struct RootView: View {
	@Bindable var graph: AppGraph
	@Environment(\.scenePhase) private var scenePhase
	@Environment(\.accessibilityReduceMotion) private var reduceMotion
	@State private var reducedComposeIsPresented = false
	@Namespace private var composeTransition
	var body: some View {
		Group {
			if graph.isRestoring {
				ProgressView()
			} else if UIDevice.current.userInterfaceIdiom == .pad, graph.routes.root == .chats {
				NavigationSplitView {
					if let store = graph.conversations, let decisions = graph.requests {
						chatsView(
							store: store,
							decisions: decisions,
							sidebar: true,
						)
					}
				} detail: {
					NavigationStack(path: Binding(get: { graph.routes.path }, set: { graph.routes.setPath($0) })) {
						ContentUnavailableView(
							String(localized: "Select a conversation", table: "Foundation"),
							systemImage: "bubble.left.and.bubble.right",
							description: Text("Choose a chat from the sidebar.", tableName: "Foundation"),
						)
						.navigationDestination(for: Route.self) { destination($0) }
					}
				}.navigationSplitViewStyle(.balanced)
			} else {
				NavigationStack(path: Binding(get: { graph.routes.path }, set: { graph.routes.setPath($0) })) {
					destination(graph.routes.root)
						.navigationDestination(for: Route.self) { destination($0) }
				}
			}
		}
		.sheet(isPresented: Binding(get: {
			graph.routes.sheet != nil && !reducedComposeIsPresented
				&& !(reduceMotion && graph.routes.sheet == .newConversation)
		}, set: { value in
			if !value, !reducedComposeIsPresented {
				graph.routes.dismissSheet()
			}
		})) {
			NavigationStack {
				if let route = graph.routes.sheet {
					destination(route)
				}
			}
			.modifier(MemberSheetSizing(route: graph.routes.sheet))
		}
		.modifier(ReducedMotionSheet<SheetPresentationKey>(item: Binding<SheetPresentationKey?>(get: {
			guard reduceMotion, graph.routes.sheet == .newConversation else { return nil }
			return SheetPresentationKey(route: .newConversation, token: graph.routes.token)
		}, set: { value in
			if value == nil {
				graph.routes.dismissSheet()
			}
		}), presentationChanged: { reducedComposeIsPresented = $0 }, sheetContent: { key in
			guard let store = graph.newConversation else { return AnyView(EmptyView()) }
			let view = NewConversationView(store: store, onClose: {
				guard graph.routes.token == key.token, graph.routes.sheet == key.route else { return }
				graph.routes.back()
			})
			return AnyView(NavigationStack { view })
		}))
		.tint(Theme.tint).messengerMotion()
		.preferredColorScheme(graph.appearance == "dark" ? .dark : graph.appearance == "light" ? .light : nil)
		.alert(
			String(localized: "Cannot open account", table: "Foundation"),
			isPresented: Binding(get: { graph.error != nil }, set: {
				if !$0 {
					graph.error = nil
				}
			}),
		) {
			Button(String(localized: "Retry", table: "Foundation")) { Task { await graph.bindSession() } }
			Button(String(localized: "Cancel", table: "Foundation"), role: .cancel) { graph.error = nil }
		} message: { Text(graph.error ?? "") }
		.task { await graph.restore() }
		.onChange(of: scenePhase) { _, value in
			if value ==
				.active
			{
				Task { await graph.session.restore(); await graph.bindSession(); await graph.push?.refreshPermission() }
			}
		}
		.onChange(of: graph.routes.token) { _, _ in Task { await graph.closeInactiveDestinations() } }
		.onChange(of: graph.session.state) { _, _ in Task { await graph.bindSession() } }
	}

	@ViewBuilder private func destination(_ route: Route) -> some View {
		switch route {
		case .chat, .reactions, .emojiPicker, .attachmentRecovery: messageDestination(route)
		case .details, .member, .permissions, .disappearing, .sharedMedia, .groupFields, .myFields: detailDestination(route)
		default: accountDestination(route)
		}
	}

	@ViewBuilder private func accountDestination(_ route: Route) -> some View {
		switch route {
		case .connect: ConnectView(store: graph.connect)
		case .chooseHandle, .changeHandle:
			if let handles = graph.handles {
				HandleView(store: handles)
			}
		case .chats, .requests: listDestination(route)
		case .settings: SettingsView(graph: graph)
		case .newConversation:
			if let store = graph.newConversation {
				NewConversationView(store: store, onClose: { graph.routes.back() })
					.modifier(ComposeDestination(namespace: composeTransition, reduceMotion: reduceMotion))
			}
		default: EmptyView()
		}
	}

	@ViewBuilder private func listDestination(_ route: Route) -> some View {
		switch route {
		case .chats:
			if let store = graph.conversations, let decisions = graph.requests {
				chatsView(store: store, decisions: decisions)
			}
		case .requests:
			if let store = graph.requestConversations, let decisions = graph.requests {
				RequestsView(store: store, decisions: decisions, onNavigate: graph.navigate)
			}
		default: EmptyView()
		}
	}

	@ViewBuilder private func messageDestination(_ route: Route) -> some View {
		switch route {
		case let .chat(id): ChatFeatureView(graph: graph, id: id)
		case let .reactions(conversationID, messageID, emoji):
			ReactionsFeatureView(graph: graph, conversationID: conversationID, messageID: messageID, emoji: emoji)
		case let .emojiPicker(conversationID, messageID):
			EmojiPickerFeatureView(graph: graph, conversationID: conversationID, messageID: messageID)
		case .attachmentRecovery: AttachmentRecoveryFeatureView(graph: graph)
		default: EmptyView()
		}
	}

	@ViewBuilder private func detailDestination(_ route: Route) -> some View {
		switch route {
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
		default: EmptyView()
		}
	}

	@ViewBuilder private func chatsView(store: ConversationsStore, decisions: RequestsStore,
	                                    sidebar: Bool = false) -> some View
	{
		#if PUSH_ENABLED
			ChatsView(
				store: store,
				decisions: decisions,
				onNavigate: { navigate($0, sidebar: sidebar) },
				serverName: graph.serverName,
				composeNamespace: composeTransition,
				selectedID: sidebar ? selectedChat : nil,
				onMute: { id, enabled in graph.push?.setConversationEnabled(enabled, id: id) },
			)
		#else
			ChatsView(
				store: store,
				decisions: decisions,
				onNavigate: { navigate($0, sidebar: sidebar) },
				serverName: graph.serverName,
				composeNamespace: composeTransition,
				selectedID: sidebar ? selectedChat : nil,
			)
		#endif
	}

	private var selectedChat: String? {
		for route in graph.routes.path.reversed() {
			if case let .chat(id) = route {
				return id
			}
		}; return nil
	}

	private func navigate(_ route: Route, sidebar: Bool) {
		if sidebar, !route.isSheet {
			graph.routes.setPath([route])
		} else {
			graph.navigate(route)
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

private struct MemberSheetSizing: ViewModifier {
	let route: Route?
	@Environment(\.dynamicTypeSize) private var dynamicTypeSize
	@State private var selection: PresentationDetent = .fraction(0.7)
	func body(content: Content) -> some View {
		if let route, case .member = route {
			content.presentationDetents([.fraction(0.7), .large], selection: $selection)
				.onAppear { selection = dynamicTypeSize.isAccessibilitySize ? .large : .fraction(0.7) }
		} else {
			content
		}
	}
}

private struct SheetPresentationKey: Hashable {
	let route: Route
	let token: UInt64
}
