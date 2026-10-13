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
	private var visibleStart = 0
	private var visibleCount = 10
	private var windowRevision: UInt64 = 0
	private var loadRevision: UInt64 = 0
	private var job: Task<Void, Never>?
	private var detailJob: Task<Void, Never>?
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
		case .loadMore: guard state.search.isEmpty, !state.loading, state.hasMore else { return }; limit += 50; load()
		case let .search(text):
			state.search = text
			visibleStart = 0
			windowRevision &+= 1
			detailJob?.cancel()
		case let .connection(copy): state.connectionBanner = copy
		case let .visible(start, count):
			visibleStart = max(0, start)
			visibleCount = max(0, count)
			windowRevision &+= 1
			detailJob?.cancel()
			let origin = DestinationOrigin(session: session, token: routes.token)
			let taskID = UUID()
			detailJob = Task { defer { tasks.removeValue(forKey: taskID) }; await hydrate(origin: origin) }
			tasks[taskID] = detailJob
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
		job?.cancel(); detailJob?.cancel(); state.loading = true; state.error = nil
		let origin = DestinationOrigin(session: session, token: routes.token), requested = limit
		let taskID = UUID()
		job = Task { [weak self] in
			guard let self else { return }
			defer { tasks.removeValue(forKey: taskID) }
			do {
				let records = try await queries.list(consent: consent, limit: requested)
				let requests = try await queries.list(consent: .unknown, limit: 501)
				try Task.checkCancellation()
				apply(origin, request: requestRevision) { state in
					// List reads may carry only IDs. Keep details until the current window refreshes.
					let existing = Dictionary(uniqueKeysWithValues: state.rows.map { ($0.id, $0.conversation) })
					state.rows = records
						.filter { $0.consent == consent && (consent != .unknown || !preferences.dismissedRequests.contains($0.id)) }
						.map { record in
							var enriched = existing[record.id] ?? record
							enriched.consent = record.consent
							return row(enriched)
						}
					state.hasMore = records.count == requested; state.loading = false
					state.requestCount = requests.count >= 501 ? nil : ListRules
						.requestCount(requests.filter { !preferences.dismissedRequests.contains($0.id) }.count)
				}
				await hydrate(origin: origin)
			} catch {
				apply(origin, request: requestRevision) { $0.loading = false; $0.error = "Cannot load conversations. Try again." }
			}
		}
		tasks[taskID] = job
	}

	private func row(_ record: ConversationRecord) -> ConversationRow {
		ConversationRow(
			conversation: record,
			title: record.title(alias: preferences.alias(for: record.id)),
			muted: preferences.muted(for: record.logicalKey(ownInboxID: ownInboxID)),
		)
	}

	private func hydrate(origin: DestinationOrigin) async {
		let revision = windowRevision
		let requestRevision = loadRevision
		let displayed = state.visibleRows
		let rows = Array(displayed[ListRules.detailWindow(
			start: visibleStart,
			count: visibleCount,
			total: displayed.count,
		)])
		for value in rows {
			guard !Task.isCancelled, revision == windowRevision, requestRevision == loadRevision, routes.admit(
				origin,
				fence: fence,
			) else { return }
			do {
				let record = try await queries.details(
					value.conversation,
					marker: preferences.marker(for: value.conversation.logicalKey(ownInboxID: ownInboxID)),
				)
				apply(origin, request: requestRevision) { state in
					guard revision == windowRevision else { return }
					if let index = state.rows.firstIndex(where: { $0.id == record.id }) {
						if record.consent == consent {
							state.rows[index] = row(record)
						} else {
							state.rows.remove(at: index)
						}
					}
				}
			} catch { continue }
		}
	}
}
