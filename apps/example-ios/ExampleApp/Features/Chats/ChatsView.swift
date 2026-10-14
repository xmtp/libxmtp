import ExampleCore
import SwiftUI

struct ChatsView: View {
	@Environment(\.dynamicTypeSize) private var dynamicTypeSize
	private var requestsTitleLayout: AnyLayout {
		dynamicTypeSize.isAccessibilitySize
			? AnyLayout(VStackLayout(alignment: .leading, spacing: 4))
			: AnyLayout(HStackLayout(spacing: 4))
	}

	@Bindable var store: ConversationsStore
	var decisions: RequestsStore
	var onNavigate: (Route) -> Void
	var serverName = ""
	var composeNamespace: Namespace.ID?
	var selectedID: String?
	var onMute: ((String, Bool) -> Void)?
	var body: some View {
		List {
			Button { onNavigate(.requests) } label: {
				HStack(spacing: 12) {
					Image(systemName: "tray")
						.font(dynamicTypeSize.isAccessibilitySize ? .system(size: 24) : .title3)
						.foregroundStyle(.tint)
					VStack(alignment: .leading, spacing: 4) {
						requestsTitleLayout {
							Text(String(localized: "chats.requests", table: "Chats")).font(.headline)
							if let count = store.state.requestCount {
								Text("· \(count)").font(dynamicTypeSize.isAccessibilitySize ? .caption : .headline)
							}
						}
						Text(String(localized: "chats.requests.detail", table: "Chats")).font(.subheadline).foregroundStyle(.secondary)
					}
					Spacer()
					Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
				}.padding(.vertical, 8)
			}
			ForEach(Array(store.state.visibleRows.enumerated()), id: \.element.id) { index, row in
				Button { onNavigate(.chat(row.id)) } label: { ConversationRowView(row: row) }
					.listRowBackground(selectedID == row.id ? Theme.tint.opacity(0.12) : Color.clear)
					.accessibilityAddTraits(selectedID == row.id ? .isSelected : [])
					.onAppear { store.send(.visible(start: index, count: 1)) }
					.swipeActions(edge: .trailing) {
						Button(String(localized: "chats.block", table: "Chats"), role: .destructive) {
							decisions.send(.requestBlock(row.id))
						}
						if let onMute {
							Button {
								onMute(row.id, row.muted)
							} label: {
								Label(
									row.muted ? String(localized: "chats.unmute", defaultValue: "Unmute", table: "Chats") : String(
										localized: "chats.mute",
										defaultValue: "Mute",
										table: "Chats",
									),
									systemImage: row.muted ? "bell" : "bell.slash",
								)
							}
						}
					}
					.contextMenu {
						Button(String(localized: "chats.block", table: "Chats"), role: .destructive) {
							decisions.send(.requestBlock(row.id))
						}
					}
			}
			if store.state.visibleRows.isEmpty, store.state.error == nil {
				ChatsEmptyState(loading: store.state.loading, searching: !store.state.search.isEmpty) {
					onNavigate(.newConversation)
				}
				.listRowSeparator(.hidden)
				.listRowBackground(Color.clear)
			} else if store.state.loading {
				ProgressView().frame(maxWidth: .infinity)
			}
			if let error = store.state.error {
				Text(error); Button(String(localized: "chats.retry", table: "Chats")) { store.send(.load) }.buttonStyle(.glass)
			}
		}
		.navigationTitle(String(localized: "chats.title", table: "Chats"))
		.navigationSubtitle(serverName)
		.safeAreaInset(edge: .top, spacing: 0) {
			if let banner = store.state.connectionBanner {
				HStack(spacing: 8) {
					if banner.hasPrefix("Connecting") || banner.hasPrefix("Reconnecting") {
						ProgressView().controlSize(.small)
					} else {
						Image(systemName: "wifi.exclamationmark")
							.font(dynamicTypeSize.isAccessibilitySize ? .system(size: 17) : .subheadline)
					}
					Text(banner).font(.subheadline)
				}
				.padding(.horizontal, 14).padding(.vertical, 10)
				.messengerGlass(in: Capsule())
				.frame(maxWidth: .infinity, alignment: .leading)
				.padding(.horizontal, 20).padding(.bottom, 12)
			}
		}
		.listStyle(.plain)
		.refreshable { await store.refresh() }
		.searchable(text: Binding(get: { store.state.search }, set: { store.send(.search($0)) }), placement: .automatic)
		.toolbar {
			ToolbarItem(placement: .topBarLeading) {
				Button { onNavigate(.settings) } label: { Image(systemName: "gearshape") }
					.accessibilityLabel(String(localized: "chats.settings", table: "Chats"))
			}
			ToolbarItem(placement: .topBarTrailing) {
				Button { onNavigate(.newConversation) } label: { Image(systemName: "square.and.pencil") }
					.accessibilityLabel(String(localized: "chats.new", table: "Chats"))
					.modifier(ComposeSource(namespace: composeNamespace))
			}
		}
		.task { store.send(.load) }
		.confirmationDialog(
			String(localized: "chats.block.confirm", table: "Chats"),
			isPresented: Binding(get: { decisions.state.confirmation != nil }, set: {
				if !$0 {
					decisions.send(.cancel)
				}
			}),
		) { Button(String(localized: "chats.block", table: "Chats"), role: .destructive) { decisions.send(.confirm) } }
	}
}

private struct ChatsEmptyState: View {
	let loading: Bool
	let searching: Bool
	let start: () -> Void
	var body: some View {
		VStack(spacing: 16) {
			if loading {
				ProgressView().controlSize(.large)
				Text(String(localized: "chats.loading", table: "Chats")).font(.headline)
			} else {
				Image(systemName: searching ? "magnifyingglass" : "bubble.left.and.bubble.right")
					.font(.largeTitle).foregroundStyle(.tint).accessibilityHidden(true)
				Text(String(localized: searching ? "chats.noMatches" : "chats.empty.title", table: "Chats"))
					.font(.title2.bold())
				Text(String(localized: searching ? "chats.noMatches.detail" : "chats.empty.detail", table: "Chats"))
					.font(.body).foregroundStyle(.secondary).multilineTextAlignment(.center)
				if !searching {
					Button(String(localized: "chats.empty.action", table: "Chats"), action: start)
						.buttonStyle(.glassProminent).padding(.top, 4)
				}
			}
		}
		.frame(maxWidth: .infinity).padding(.horizontal, 24).padding(.vertical, 48)
	}
}

struct ConversationRowView: View {
	let row: ConversationRow
	var body: some View {
		HStack(alignment: .center, spacing: 12) {
			Avatar(seed: row.id, label: row.initials)
			VStack(alignment: .leading, spacing: 4) {
				HStack(alignment: .firstTextBaseline) {
					ConversationTitle(title: row.title).font(.headline)
					Spacer(minLength: 8)
					if let date = row.conversation.lastActivity {
						Text(date, format: .dateTime.hour().minute()).font(.caption).foregroundStyle(.secondary)
					}
				}
				HStack {
					Text(row.conversation.preview).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
					Spacer(minLength: 8)
					if row.muted {
						Image(systemName: "bell.slash").font(.caption).foregroundStyle(.secondary).accessibilityLabel(String(
							localized: "chats.muted",
							table: "Chats",
						))
					}
					if row.conversation.unread > 0 {
						Text("\(row.conversation.unread)").font(.caption2.bold()).foregroundStyle(.white).padding(.horizontal, 6).padding(
							.vertical,
							3,
						).background(.tint, in: Capsule())
					}
				}
			}
		}.padding(.vertical, 5).foregroundStyle(.primary)
	}
}

private struct ComposeSource: ViewModifier {
	let namespace: Namespace.ID?
	func body(content: Content) -> some View {
		if let namespace {
			content.matchedTransitionSource(id: "new-message", in: namespace)
		} else {
			content
		}
	}
}
