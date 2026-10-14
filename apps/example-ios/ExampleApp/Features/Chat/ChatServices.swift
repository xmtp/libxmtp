import CryptoKit
import ExampleCore
import ExampleSDK
import Observation
import SwiftUI

@MainActor final class ChatServices {
	private static var current: ChatServices?
	let key: SessionKey
	let queries: SDKChatQueries
	let gateway: SDKAttachmentGateway
	let staging: AttachmentStagingFiles
	let sends: SendDraftStore
	let composers: ComposerDraftStore
	let files: AttachmentDraftStore
	let coordinator: SendCoordinator
	let cache = TranscriptCache()
	static func get(_ graph: AppGraph) throws -> ChatServices {
		guard let active = graph.active else { throw ChatError.stale }
		if let current, current.key == active.key {
			return current
		}
		let value = try ChatServices(graph)
		current = value
		graph.session.scope.addFinalCleanup { [weak value] in
			guard let value else { return }
			value.cache.clear()
			if current === value {
				current = nil
			}
		}
		return value
	}

	private init(_ graph: AppGraph) throws {
		guard let active = graph.active, let client = graph.factory.client(for: active.key),
		      let preferences = graph.preferences else { throw ChatError.stale }
		key = active
			.key
		queries = SDKChatQueries(client: client, limiter: graph.reads)
		gateway = SDKAttachmentGateway(client: client)
		staging = try AttachmentStagingFiles(temporaryDirectory: graph.files.paths(active.key.profileID).root
			.appendingPathComponent(
				"temp",
				isDirectory: true,
			))
		sends = SendDraftStore(
			read: { try preferences.read([SendDraft].self, named: "send-drafts") ?? [] },
			write: { try preferences.write($0, named: "send-drafts") },
		)
		composers = ComposerDraftStore(
			read: { try preferences.read(ComposerDraft.self, named: Self.composerName($0)) },
			write: { try preferences.write($1, named: Self.composerName($0)) },
		)
		files = AttachmentDraftStore(
			names: { try preferences.names(prefix: "attachment-").filter { $0 != "attachment-refs" } },
			read: { try preferences.read(FileDraft.self, named: $0) },
			write: { try preferences.write($1, named: $0) },
			remove: { try preferences.remove(named: $0) },
			references: { try preferences.read([String].self, named: "attachment-refs") ?? [] },
			saveReferences: { try preferences.write($0, named: "attachment-refs") },
		)
		coordinator = SendCoordinator(drafts: sends, queries: queries, accepts: { graph.fence.accepts(active.key) })
	}

	private static func composerName(_ id: String) -> String {
		"composer-" + SHA256.hash(data: Data(id.utf8)).map { String(
			format: "%02x",
			$0,
		) }.joined()
	}
}

@MainActor @Observable final class ChatRuntime {
	let timeline: ChatTimelineStore
	let composer: ComposerStore
	let actions: MessageActionStore
	let attachments: AttachmentStore
	let services: ChatServices
	let origin: DestinationOrigin
	let cleanupKey = UUID().uuidString
	let imports = PhotoImportController()
	init(graph: AppGraph, id: String) throws {
		let services = try ChatServices.get(graph)
		try services.coordinator.recoverAccepted()
		self.services = services
		let origin = DestinationOrigin(session: services.key, token: graph.routes.token)
		self.origin = origin
		let valid: (DestinationOrigin) -> Bool = { graph.routes.admit($0, fence: graph.fence) }
		timeline = Self.makeTimeline(graph: graph, id: id, services: services, origin: origin)
		let timeline = timeline
		composer = ComposerStore(
			id: id,
			origin: origin,
			drafts: services.composers,
			sends: services.sends,
			coordinator: services.coordinator,
			admit: valid,
			accepted: { await timeline.refresh() },
		)
		actions = MessageActionStore(
			queries: services.queries,
			coordinator: services.coordinator,
			origin: origin,
			admit: valid,
			own: graph.active?.profile.inboxID ?? "",
			changed: { await timeline.refresh() },
		)
		attachments = AttachmentStore(
			gateway: services.gateway,
			staging: services.staging,
			drafts: services.files,
			coordinator: services.coordinator,
			conversationID: id,
			origin: origin,
			admit: valid,
			allowed: { timeline.state.conversation?.consent == .allowed },
			changed: { await timeline.refresh() },
			authorizeDownload: { try await services.queries.conversation(id: id).consent == .allowed },
		)
	}

	private static func makeTimeline(
		graph: AppGraph,
		id: String,
		services: ChatServices,
		origin: DestinationOrigin,
	) -> ChatTimelineStore {
		let preferences = graph.preferences
		return ChatTimelineStore(
			id: id,
			origin: origin,
			queries: services.queries,
			cache: services.cache,
			admit: { graph.routes.admit($0, fence: graph.fence) },
			readAnchor: {
				do { return try preferences?.read([String: ScrollAnchor].self, named: "scroll-anchors")?[id] } catch { return nil }
			},
			saveAnchor: { anchor in
				var anchors = try preferences?
					.read([String: ScrollAnchor].self, named: "scroll-anchors") ?? [:]
				anchors[id] = anchor
				try preferences?.write(
					anchors,
					named: "scroll-anchors",
				)
			},
			markRead: { record, marker in
				let logicalKey = record.logicalKey(ownInboxID: graph.active?.profile.inboxID ?? "")
				try preferences?.setMarker(marker, for: logicalKey)

			},
		)
	}
}
