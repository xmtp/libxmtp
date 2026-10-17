import ExampleCore
import SwiftUI

struct SharedMediaFeatureView: View {
	@State private var cleanupKey = UUID().uuidString
	let graph: AppGraph
	let id: String
	@State private var media: SharedMediaStore?
	@State private var files: AttachmentStore?
	@State private var failure: String?
	var body: some View {
		List {
			Section {
				Text("Download a file to verify it before you open or save it.", tableName: "SharedMedia")
					.foregroundStyle(.secondary)
			}
			if let files, let media {
				Section(String(localized: "Photos and videos", table: "SharedMedia")) {
					LazyVGrid(columns: [GridItem(.adaptive(minimum: 140))], spacing: 12) {
						ForEach(media.state.rows.filter(isVisual)) { row in
							mediaCard(row, files: files, allowed: media.state.allowed)
						}
					}
				}
				Section(String(localized: "Files", table: "SharedMedia")) {
					ForEach(media.state.rows.filter { !isVisual($0) }) { row in
						mediaCard(row, files: files, allowed: media.state.allowed)
					}
				}
				if let error = media.state.error {
					Text(String(localized: String.LocalizationValue(error), table: "SharedMedia")).foregroundStyle(.red)
				}
			}
			if let failure {
				Text(String(localized: String.LocalizationValue(failure), table: "SharedMedia")).foregroundStyle(.red)
			}
			Section {
				Text("Only the latest 50 file messages load here.", tableName: "SharedMedia").font(.caption)
					.foregroundStyle(.secondary)
			}
		}.navigationTitle(String(localized: "Media and files", table: "SharedMedia")).task(id: graph.routes.token) {
			let token = graph.routes.token
			await graph.session.scope.runCleanup(for: cleanupKey); media = nil; files = nil
			guard !Task.isCancelled, graph.routes.token == token, graph.routes.current == .sharedMedia(id) else { return }
			PreviewImages.shared.clear()
			do {
				let services = try ChatServices.get(graph)
				let origin = DestinationOrigin(session: services.key, token: graph.routes.token)
				let media = SharedMediaStore(
					id: id,
					queries: services.queries,
					origin: origin,
					admit: { graph.routes.admit($0, fence: graph.fence) },
				)
				let files = AttachmentStore(
					gateway: services.gateway,
					staging: services.staging,
					drafts: services.files,
					coordinator: services.coordinator,
					conversationID: id,
					origin: origin,
					admit: { graph.routes.admit($0, fence: graph.fence) },
					allowed: { media.state.allowed },
					authorizeDownload: { try await services.queries.conversation(id: id).consent == .allowed },
				)
				self.media = media; self.files = files
				cleanupKey = UUID().uuidString; graph.session.scope
					.setCleanup(for: cleanupKey) { await media.close(); await files.close() }
				media.send(.load)
			} catch { failure = "Cannot load media and files. Try again." }
		}
		.onChange(of: graph.consentRevision) { _, _ in
			PreviewImages.shared.clear(); files?.invalidateDownloads(); media?.send(.load)
		}
		.onDisappear {
			PreviewImages.shared
				.clear(); let key = cleanupKey; Task { await graph.session.scope.runCleanup(for: key) }
		}
	}

	private func isVisual(_ row: ChatMessage) -> Bool {
		guard case let .attachment(file) = row.content else { return false }
		return ["jpg", "jpeg", "png", "gif", "webp", "heic", "mov", "mp4"]
			.contains(URL(fileURLWithPath: file.filename ?? "").pathExtension.lowercased())
	}

	@ViewBuilder private func mediaCard(_ row: ChatMessage, files: AttachmentStore, allowed: Bool) -> some View {
		if case let .attachment(descriptor) = row.content {
			VStack(alignment: .leading, spacing: 8) {
				HStack { IdentityText(inboxID: row.sender, handle: graph.identityDirectory?.identities[row.sender]?.handle); Text(
					row.date,
					format: .dateTime.month().day().hour().minute(),
				) }.font(.caption).foregroundStyle(.secondary)
				AttachmentCardView(id: row.id, descriptor: descriptor, store: files, graph: graph, allowed: allowed)
				Button(String(localized: "View in chat", table: "SharedMedia")) { viewInChat(row) }.buttonStyle(.glass)
			}
		}
	}

	private func viewInChat(_ row: ChatMessage) {
		do {
			if let position = row.position, let preferences = graph.preferences {
				var anchors = try preferences.read(
					[String: ScrollAnchor].self,
					named: "scroll-anchors",
				) ?? [:]; anchors[id] = ScrollAnchor(messageID: row.id, position: position); try preferences.write(
					anchors,
					named: "scroll-anchors",
				)
			}
			graph.navigate(.chat(id))
		} catch { failure = "Cannot open this position. Try again." }
	}
}
