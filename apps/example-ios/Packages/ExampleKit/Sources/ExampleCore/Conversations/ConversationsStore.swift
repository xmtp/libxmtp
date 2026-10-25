import Foundation
import Observation

public struct ConversationsState: Equatable, Sendable {
	public var rows: [ConversationRow] = []
	public var search = ""
	public var loading = false
	public var hasMore = true
	public var requestCount: Int?
	public var error: String?
	public var connectionBanner: String?
	public var visibleRows: [ConversationRow] {
		search.isEmpty ? rows : rows.filter { $0.matches(search) }
	}
}

public enum ConversationsAction: Sendable { case load, loadMore, search(String), visible(start: Int, count: Int),
                                                 connection(String?)
}

@MainActor @Observable public final class ConversationsStore {
	public private(set) var state = ConversationsState()
	private let session: SessionKey
	private let fence: SessionFence
	private let routes: RouteOwner
	private let queries: any ConversationQueries
	private let preferences: any ConversationPreferences
	private let ownInboxID: String
	private let consent: ConversationConsent
	private var limit = 50
	/// The SDK rejects a conversation list limit above this.
	private static let maxListLimit = 1000
	private var visibleStart = 0
	private var visibleCount = 10
	private var windowRevision: UInt64 = 0
	private var loadRevision: UInt64 = 0
	private var job: Task<Void, Never>?
	private var detailJob: Task<Void, Never>?
	private var pendingDetailIDs: [String] = []
	private var queuedDetailIDs: Set<String> = []
	private var hydratedDetailIDs: Set<String> = []
	private var tasks: [UUID: Task<Void, Never>] = [:]
	public init(
		session: SessionKey,
		fence: SessionFence,
		routes: RouteOwner,
		queries: any ConversationQueries,
		preferences: any ConversationPreferences,
		ownInboxID: String,
		consent: ConversationConsent = .allowed,
	) {
		self.session = session; self.fence = fence; self.routes = routes; self.queries = queries; self
			.preferences = preferences; self.ownInboxID = ownInboxID; self.consent = consent
	}

	public func send(_ action: ConversationsAction) {
		switch action {
		case .load: limit = 50; load()
		case .loadMore:
			guard state.search.isEmpty, !state.loading, state.hasMore else { return }
			limit = min(limit + 50, Self.maxListLimit)
			load()
		case let .search(text):
			state.search = text
			visibleStart = 0
			resetHydration()
			scheduleHydration(origin: DestinationOrigin(session: session, token: routes.token))
		case let .connection(copy): state.connectionBanner = copy
		case let .visible(start, count):
			visibleStart = max(0, start)
			visibleCount = max(0, count)
			let origin = DestinationOrigin(session: session, token: routes.token)
			scheduleHydration(origin: origin)
			if ListRules.shouldPage(lastVisible: start + max(0, count - 1), loaded: state.rows.count, search: state.search) {
				send(.loadMore)
			}
		}
	}

	public func waitForLoad() async {
		await job?.value
		await detailJob?.value
	}

	public func refresh() async {
		load(); await job?.value
	}

	public func close() async {
		let pending = Array(tasks.values)
		resetHydration()
		pending.forEach { $0.cancel() }
		for task in pending {
			await task.value
		}
	}

	private func apply(_ origin: DestinationOrigin, request: UInt64? = nil, _ body: (inout ConversationsState) -> Void) {
		guard request == nil || request == loadRevision, routes.admit(origin, fence: fence) else { return }; body(&state)
	}

	private func load() {
		loadRevision &+= 1
		let requestRevision = loadRevision
		let pendingVisibleIDs = state.visibleRows.filter { queuedDetailIDs.contains($0.id) }.map(\.id)
		job?.cancel(); resetHydration(); state.loading = true; state.error = nil
		let origin = DestinationOrigin(session: session, token: routes.token)
		// Dismissed requests still occupy the SDK prefix. Fetch enough for the visible page.
		let requested = min(
			limit + (consent == .unknown ? preferences.dismissedRequests.count : 0),
			Self.maxListLimit,
		)
		let taskID = UUID()
		job = Task { [weak self] in
			guard let self else { return }
			defer { tasks.removeValue(forKey: taskID) }
			do {
				try Task.checkCancellation()
				guard routes.admit(origin, fence: fence) else { throw CancellationError() }
				let records = try await queries.list(consent: consent, limit: requested)
				let requests = try await countRecords(origin: origin)
				try Task.checkCancellation()
				apply(origin, request: requestRevision) { state in
					// List reads may carry only IDs. Keep details until the current window refreshes.
					let existing = Dictionary(uniqueKeysWithValues: state.rows.map { ($0.id, $0.conversation) })
					let visibleRecords = records
						.filter { $0.consent == consent && (consent != .unknown || !preferences.dismissedRequests.contains($0.id)) }
					state.rows = visibleRecords.prefix(limit)
						.map { record in
							var enriched = existing[record.id] ?? record
							enriched.consent = record.consent
							return row(enriched)
						}
					hydratedDetailIDs.formIntersection(state.rows.map(\.id))
					state.hasMore = limit < Self.maxListLimit
						&& (records.count == requested || visibleRecords.count > limit)
					state.loading = false
					state.requestCount = requests.flatMap { records in
						records.count >= 501 ? nil : ListRules
							.requestCount(records.count(where: { !preferences.dismissedRequests.contains($0.id) }))
					}
				}
				scheduleHydration(origin: origin, retaining: pendingVisibleIDs)
				await detailJob?.value
			} catch {
				apply(origin, request: requestRevision) { $0.loading = false; $0.error = "Cannot load conversations. Try again." }
			}
		}
		tasks[taskID] = job
	}

	private func countRecords(origin: DestinationOrigin) async throws -> [ConversationRecord]? {
		// Only Chats displays the request badge; Requests already fetched its primary list.
		guard consent != .unknown else { return nil }
		do {
			try Task.checkCancellation()
			guard routes.admit(origin, fence: fence) else { throw CancellationError() }
			return try await queries.list(consent: .unknown, limit: 501)
		} catch is CancellationError {
			throw CancellationError()
		} catch {
			return nil
		}
	}

	private func row(_ record: ConversationRecord) -> ConversationRow {
		ConversationRow(
			conversation: record,
			title: record.title(alias: preferences.alias(for: record.id)),
			muted: preferences.muted(for: record.logicalKey(ownInboxID: ownInboxID)),
		)
	}

	private func resetHydration() {
		windowRevision &+= 1
		detailJob?.cancel()
		detailJob = nil
		pendingDetailIDs.removeAll()
		queuedDetailIDs.removeAll()
	}

	private func scheduleHydration(origin: DestinationOrigin, retaining ids: [String] = []) {
		guard routes.admit(origin, fence: fence) else { return }
		let displayed = state.visibleRows
		let displayedIDs = Set(displayed.map(\.id))
		for id in ids where displayedIDs.contains(id) && queuedDetailIDs.insert(id).inserted {
			pendingDetailIDs.append(id)
		}
		for value in displayed[ListRules.detailWindow(start: visibleStart, count: visibleCount, total: displayed.count)]
			where queuedDetailIDs.insert(value.id).inserted
		{
			pendingDetailIDs.append(value.id)
		}
		if !state.search.isEmpty {
			// Search cannot discover a field until it is read. Stay inside the loaded detail window.
			let window = ListRules.detailWindow(start: visibleStart, count: visibleCount, total: state.rows.count)
			for value in state.rows[window]
				where !hydratedDetailIDs.contains(value.id) && queuedDetailIDs.insert(value.id).inserted
			{
				pendingDetailIDs.append(value.id)
			}
		}
		guard detailJob == nil, !pendingDetailIDs.isEmpty else { return }
		let taskID = UUID(), revision = windowRevision
		detailJob = Task {
			defer {
				tasks.removeValue(forKey: taskID)
				if revision == windowRevision {
					detailJob = nil
				}
			}
			await hydrate(origin: origin)
		}
		tasks[taskID] = detailJob
	}
}

private extension ConversationsStore {
	func hydrate(origin: DestinationOrigin) async {
		let revision = windowRevision
		let requestRevision = loadRevision
		while !pendingDetailIDs.isEmpty {
			guard !Task.isCancelled, revision == windowRevision, requestRevision == loadRevision, routes.admit(
				origin,
				fence: fence,
			) else { return }
			let id = pendingDetailIDs.removeFirst()
			guard let value = state.rows.first(where: { $0.id == id }) else {
				queuedDetailIDs.remove(id)
				continue
			}
			do {
				let record = try await queries.details(
					value.conversation,
					marker: preferences.marker(for: value.conversation.logicalKey(ownInboxID: ownInboxID)),
				)
				try Task.checkCancellation()
				apply(origin, request: requestRevision) { state in
					guard revision == windowRevision else { return }
					if let index = state.rows.firstIndex(where: { $0.id == record.id }) {
						if record.consent == consent {
							state.rows[index] = row(record)
							hydratedDetailIDs.insert(record.id)
						} else {
							state.rows.remove(at: index)
							hydratedDetailIDs.remove(record.id)
						}
					}
				}
			} catch {}
			if revision == windowRevision {
				queuedDetailIDs.remove(id)
			}
		}
	}
}
