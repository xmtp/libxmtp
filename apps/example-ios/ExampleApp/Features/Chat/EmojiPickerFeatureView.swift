import ExampleCore
import SwiftUI

struct EmojiPickerFeatureView: View {
	@State private var cleanupKey = UUID().uuidString
	let graph: AppGraph; let conversationID: String; let messageID: String
	@State private var actions: MessageActionStore?
	@State private var search = ""; @State private var failure: String?
	@State private var catalog: [(String, String)] = []
	private static var recent: [String] = []
	private struct EmojiSection { let title: String; var values: [(String, String)] }
	private var sections: [EmojiSection] {
		let boundaries = ["🐵": "Animals and nature", "🍇": "Food and drink", "🌍": "Travel and places",
		                  "🎃": "Activities", "👓": "Objects", "🏧": "Symbols", "🏁": "Flags"]
		var result = [EmojiSection(title: "Smileys and people", values: [])]
		for item in catalog {
			if let title = boundaries[item.0] {
				result.append(EmojiSection(title: title, values: []))
			}
			result[result.count - 1].values.append(item)
		}
		return result
	}

	private var filtered: [(String, String)] {
		search.isEmpty ? catalog : catalog.filter { $0.1.localizedCaseInsensitiveContains(search) || $0.0.contains(search) }
	}

	var body: some View {
		NavigationStack {
			VStack(spacing: 0) {
				HStack(spacing: 8) {
					Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
					TextField("Search emoji", text: $search).textInputAutocapitalization(.never).autocorrectionDisabled()
						.accessibilityIdentifier("emoji.search")
				}.padding(10).background(.quaternary, in: RoundedRectangle(cornerRadius: 10)).padding(.horizontal, 16).padding(
					.vertical,
					12,
				)
				ScrollView { VStack(alignment: .leading, spacing: 20) {
					if search.isEmpty,
					   !Self.recent.isEmpty
					{
						Text("Recently used", tableName: "Chat").font(.headline); grid(Self.recent.map { ($0, $0) })
					}
					if search.isEmpty {
						ForEach(Array(sections.enumerated()), id: \.offset) { _, section in
							VStack(alignment: .leading, spacing: 12) {
								Text(String(localized: String.LocalizationValue(section.title), table: "Chat"))
									.font(.subheadline.weight(.semibold))
								grid(section.values)
							}
						}
					} else {
						grid(filtered)
					}
					if let failure {
						Text(String(localized: String.LocalizationValue(failure), table: "Chat")).foregroundStyle(.red)
					}
				}.padding(.horizontal, 16).padding(.bottom, 16) }
			}.navigationTitle(String(localized: "Choose a reaction", table: "Chat"))
				.navigationBarTitleDisplayMode(.inline)
				.toolbar {
					ToolbarItem(placement: .cancellationAction) {
						Button(String(localized: "Close", table: "Chat"), systemImage: "xmark") { graph.routes.back() }
							.labelStyle(.iconOnly).buttonStyle(.glass).accessibilityIdentifier("emoji.close")
					}
				}
		}.presentationDetents([.medium, .large])
			.task(id: graph.routes.token) {
				let token = graph.routes.token
				let destination = Route.emojiPicker(conversationID: conversationID, messageID: messageID)
				await graph.session.scope.runCleanup(for: cleanupKey); actions = nil
				guard !Task.isCancelled, graph.routes.token == token, graph.routes.current == destination else { return }
				load(); prepare()
			}.onDisappear { let key = cleanupKey; Task { await graph.session.scope.runCleanup(for: key) } }
	}

	private func grid(_ values: [(String, String)]) -> some View {
		LazyVGrid(
			columns: Array(repeating: GridItem(.flexible(minimum: 44), spacing: 0), count: 8),
			spacing: 0,
		) {
			ForEach(Array(values.enumerated()), id: \.offset) { _, item in
				Button(item.0) { choose(item.0) }.font(.system(size: 28)).frame(
					minWidth: 44,
					minHeight: 44,
				).buttonStyle(.plain).accessibilityLabel(item.1).accessibilityIdentifier("emoji.option." + item.0)
			}
		}
	}

	private func load() {
		guard let url = Bundle.main.url(forResource: "emoji-search", withExtension: "tsv") else { return }
		let text: String
		do { text = try String(contentsOf: url, encoding: .utf8) } catch { failure = "Cannot load emoji names."; return }
		catalog = text.split(separator: "\n").compactMap { line in let parts = line.split(
			separator: "\t",
			maxSplits: 1,
		); return parts.count == 2 ? (String(parts[0]), String(parts[1])) : nil }
	}

	private func prepare() {
		do { let services = try ChatServices.get(graph); let origin = DestinationOrigin(
			session: services.key,
			token: graph.routes.token,
		); let value = MessageActionStore(
			queries: services.queries,
			coordinator: services.coordinator,
			origin: origin,
			admit: { graph.routes.admit($0, fence: graph.fence) },
			own: graph.active?.profile.inboxID ?? "",
			changed: {},
		); actions = value; cleanupKey = UUID().uuidString; graph.session.scope
			.setCleanup(for: cleanupKey) { await value.close() }; value.send(.load(messageID))
		} catch {
			failure = "Cannot open message actions."
		}
	}

	private func choose(_ emoji: String) {
		guard let actions, let row = actions.state.message, actions.allowed else { return }
		actions.send(.react(row, emoji))
		Task {
			await actions.wait(); if let error = actions.state.error {
				failure = error; return
			}; Self.recent
				.removeAll { $0 == emoji }; Self.recent.insert(
					emoji,
					at: 0,
				); Self.recent = Array(Self.recent.prefix(30)); graph.routes.back()
		}
	}
}
