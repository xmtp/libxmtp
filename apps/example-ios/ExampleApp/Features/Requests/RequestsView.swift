import ExampleCore
import SwiftUI

struct RequestsView: View {
	@Bindable var store: ConversationsStore
	@Bindable var decisions: RequestsStore
	var onNavigate: (Route) -> Void
	var body: some View {
		List {
			Section {
				if let error = store.state.error {
					VStack(spacing: 12) {
						Text(error).foregroundStyle(.secondary)
						Button(copy("requests.retry")) { store.send(.load) }.disabled(store.state.loading)
					}.frame(maxWidth: .infinity).padding(.vertical, 24).listRowSeparator(.hidden)
				} else if store.state.visibleRows.isEmpty {
					VStack(spacing: 12) {
						if store.state.loading {
							ProgressView()
						}
						Text(String(localized: store.state.loading ? "requests.loading" : "requests.empty.title", table: "Requests"))
							.font(.headline)
						if !store.state.loading {
							Text(String(localized: "requests.empty.detail", table: "Requests")).foregroundStyle(.secondary)
								.multilineTextAlignment(.center)
						}
					}.frame(maxWidth: .infinity).padding(.vertical, 40).listRowSeparator(.hidden)
				}
				ForEach(Array(store.state.visibleRows.enumerated()), id: \.element.id) { index, row in
					Button { onNavigate(.chat(row.id)) } label: { ConversationRowView(row: row) }
						.onAppear { store.send(.visible(start: index, count: 1)) }
						.swipeActions {
							Button(String(localized: "requests.delete", table: "Requests"), role: .destructive) {
								decisions.send(.requestDelete(row.id))
							}; Button(
								String(localized: "requests.block", table: "Requests"),
								role: .destructive,
							) { decisions.send(.requestBlock(row.id)) }
						}
						.contextMenu {
							Button(String(localized: "requests.accept", table: "Requests")) { decisions.send(.accept(row.id)) }; Button(
								String(localized: "requests.delete", table: "Requests"),
								role: .destructive,
							) { decisions.send(.requestDelete(row.id)) }; Button(
								String(localized: "requests.block", table: "Requests"),
								role: .destructive,
							) { decisions.send(.requestBlock(row.id)) }
						}
				}
			} header: { Text(String(localized: "requests.detail", table: "Requests")).font(.body).textCase(nil) } footer: {
				Text(String(
					localized: "requests.footer",
					table: "Requests",
				))
			}
			if let error = decisions.state.error {
				Text(error)
			}
		}.listStyle(.plain).navigationTitle(String(localized: "requests.title", table: "Requests"))
			.refreshable { await store.refresh() }.task { store.send(.load) }
			.confirmationDialog(confirmationTitle, isPresented: confirmation) {
				Button(confirmationAction, role: .destructive) { decisions.send(.confirm) }
			} message: { Text(confirmationMessage) }
	}

	private var isBlock: Bool {
		if case .block? = decisions.state.confirmation {
			return true
		}
		return false
	}

	private var confirmationTitle: String {
		copy(isBlock ? "requests.block.title" : "requests.delete.title")
	}

	private var confirmationAction: String {
		copy(isBlock ? "requests.block" : "requests.delete")
	}

	private var confirmationMessage: String {
		copy(isBlock ? "requests.block.detail" : "requests.delete.detail")
	}

	private func copy(_ key: String) -> String {
		String(localized: String.LocalizationValue(key), table: "Requests")
	}

	private var confirmation: Binding<Bool> {
		Binding(get: { decisions.state.confirmation != nil }, set: {
			if !$0 {
				decisions.send(.cancel)
			}
		})
	}
}
