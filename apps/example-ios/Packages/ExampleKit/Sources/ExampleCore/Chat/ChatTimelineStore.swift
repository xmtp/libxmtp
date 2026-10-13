import Foundation
import Observation

public struct ChatTimelineState: Equatable, Sendable {
	public var conversation: ConversationRecord?
	public var window = HistoryWindow()
	public var pending: [ChatMessage] =
		[]
	public var pendingBefore: RecoveryPosition?
	public var morePending = false
	public var loading =
		false
	public var error: String?
	public var target: String?
	public var anchorID: String?
	public var atNewest = true
	public var messages: [ChatMessage] {
		window.rows.filter { $0.content != .reaction }.reversed() + pending.filter { $0.content != .reaction }
	}
}

public enum ChatTimelineAction: Sendable {
	case load, older, olderPending, newestPending, newest, visible(
		String?,
		newest: Bool,
		foreground: Bool,
	), visibleMessages([String], foreground: Bool), refresh
}

@MainActor @Observable public final class ChatTimelineStore {
	public private(set) var state = ChatTimelineState()
	private let id: String
	private let origin: DestinationOrigin
	private let queries: any ChatQueries
	private let admit: @MainActor (DestinationOrigin) -> Bool
	private let cache: TranscriptCache
	private let readAnchor: () -> ScrollAnchor?
	private let saveAnchor: (ScrollAnchor) throws
		-> Void
	private let markRead: (ConversationRecord, Int64) throws -> Void
	private var job: Task<Void, Never>?
	private let jobs = InFlightTasks()
	public init(
		id: String,
		origin: DestinationOrigin,
		queries: any ChatQueries,
		cache: TranscriptCache = TranscriptCache(),
		admit: @escaping @MainActor (DestinationOrigin) -> Bool,
		readAnchor: @escaping () -> ScrollAnchor? = { nil },
		saveAnchor: @escaping (ScrollAnchor) throws -> Void = { _ in },
		markRead: @escaping (ConversationRecord, Int64) throws -> Void = { _, _ in },
	) {
		self.id = id
		self.origin = origin
		self.queries = queries
		self.cache = cache
		self.admit = admit
		self
			.readAnchor = readAnchor
		self.saveAnchor = saveAnchor
		self.markRead = markRead
	}

	public func send(_ action: ChatTimelineAction) {
		guard admit(origin) else { return }
		switch action {
		case let .visibleMessages(ids, foreground):
			let visible = Set(ids)
			let anchor = state.messages.first { visible.contains($0.id) }?.id
			let newest = state.messages.last.map { visible.contains($0.id) } ?? false
			send(.visible(anchor, newest: newest, foreground: foreground))
		case let .visible(id, newest, foreground):
			state.atNewest = newest
			if let id {
				state.anchorID = id
			}
			do {
				if let row = state.window.rows.first(where: { $0.id == id }),
				   let position = row.position
				{
					try saveAnchor(ScrollAnchor(
						messageID: row.id,
						position: position,
					))
				}
				if foreground, newest, state.window.atNewest, admit(origin),
				   let marker = state.window.rows.map(\.insertedAt).max()
				{
					if let conversation = state.conversation {
						try markRead(conversation, marker)
					}
				}
			} catch { state.error = "Cannot save your reading position." }
		default:
			job?.cancel()
			state.loading = true
			job = jobs.start { await self.run(action) }
		}
	}

	public func refresh() async {
		await run(.refresh)
	}

	public func wait() async {
		await job?.value
	}

	public func close() async {
		await jobs.close()
	}

	private func apply(_ body: (inout ChatTimelineState) -> Void) {
		guard admit(origin) else { return }
		body(&state)
	}

	private func run(_ action: ChatTimelineAction) async {
		do {
			let conversation = try await queries.conversation(id: id)
			let pages = HistoryPages(queries: queries)
			if case .olderPending = action {
				try await loadPending(before: state.pendingBefore)
				apply { $0.loading = false }
				return
			}
			if case .newestPending = action {
				try await loadPending(before: nil)
				apply { $0.loading = false }
				return
			}
			var window = try await historyWindow(action, pages: pages)
			try Task.checkCancellation()
			guard admit(origin) else { return }
			if case .refresh = action {
				window = cache.refresh(id, page: window, anchor: state.anchorID)
			} else {
				window = cache.put(
					id,
					window: window,
					anchor: state.anchorID,
				)
			}
			apply { $0.conversation = conversation
				$0.window = window
				$0.loading = false
				$0.error = nil
			}
			try await loadPending(before: nil)
		} catch is CancellationError {} catch { apply { $0.loading = false
			$0.error = "Message updates stopped."
		} }
	}

	private func historyWindow(_ action: ChatTimelineAction, pages: HistoryPages) async throws -> HistoryWindow {
		if case .older = action {
			let page = try await pages.older(id: id, before: state.window.last)
			guard admit(origin) else { throw ChatError.stale }
			return cache.append(id, page: page, anchor: state.window.rows.last?.id)
		}
		if case .load = action, let anchor = readAnchor() {
			var window = try await pages.around(id: id, anchor: anchor.position)
			let target = AnchorRestore.target(anchor, rows: window.rows)
			window.changed = target.changed
			if target.changed {
				window.notice = "Position changed"
			}
			apply { $0.target = target.id }
			return window
		}
		if case .refresh = action, !state.window.atNewest || !state.atNewest,
		   let first = state.window.rows.first(where: { $0.id == state.anchorID })?.position ?? state.window.first
		{
			return try await pages.around(id: id, anchor: first)
		}
		return try await pages.older(id: id)
	}

	private func loadPending(before: RecoveryPosition?) async throws {
		var cursor = before
		var skipped: UInt32 = 0
		for _ in 0 ..< 4 {
			let page = try await queries.pending(id: id, before: cursor)
			skipped += page.skipped
			if !page.rows.isEmpty || !page.hasMore {
				apply {
					$0.pending = page.rows
					$0.pendingBefore = page.last
					$0.morePending = page
						.hasMore
					if skipped >
						0
					{
						$0.error = page
							.hasMore ? "Some stored pending messages cannot be read. Load more to continue." :
							"Some stored pending messages cannot be read. Refresh to try again."
					}
				}
				return
			}
			cursor = page.last
			if cursor == nil {
				throw ChatError.unavailable
			}
		}
		apply {
			$0.pendingBefore = cursor
			$0
				.morePending = true
			if skipped >
				0
			{
				$0.error = "Some stored pending messages cannot be read. Load more to continue."
			}
		}
	}
}
