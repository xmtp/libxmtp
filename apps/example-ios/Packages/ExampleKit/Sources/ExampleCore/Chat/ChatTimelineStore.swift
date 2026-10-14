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
		var published = window.rows.filter { $0.content != .reaction }; published.reverse()
		var queued = pending.filter { $0.content != .reaction }; queued.reverse()
		return published + queued
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
	private var pendingRevision: UInt64 = 0
	private let jobs = InFlightTasks()
	private var recovery: ReadingRecovery?
	private var recoveryMessage: String?
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
			receiveVisibility(ids, foreground: foreground)
		case let .visible(id, newest, foreground):
			updateReadingPosition(id, newest: newest, foreground: foreground)
		default:
			if case .newest = action {
				state.target = nil
				state.atNewest = true
				recovery = nil
				recoveryMessage = nil
			}
			job?.cancel()
			pendingRevision += 1
			let revision = pendingRevision
			state.loading = true
			job = jobs.start { await self.run(action, revision: revision) }
		}
	}

	private func receiveVisibility(_ ids: [String], foreground: Bool) {
		let visible = Set(ids)
		guard !visible.isEmpty else { return }
		if let target = state.target {
			guard visible.contains(target) else { return }
			state.target = nil
		}
		let anchor = state.messages.first { visible.contains($0.id) && !$0.content.isDeleted }?.id
		let newest = state.messages.last.map { visible.contains($0.id) } ?? false
		send(.visible(anchor, newest: newest, foreground: foreground))
	}

	private func updateReadingPosition(_ id: String?, newest: Bool, foreground: Bool) {
		if let id, state.window.rows.first(where: { $0.id == id })?.content.isDeleted == true {
			return
		}
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
	}

	public func refresh() async {
		send(.refresh)
		await job?.value
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
}

private extension ChatTimelineStore {
	private func run(_ action: ChatTimelineAction, revision: UInt64) async {
		do {
			recoveryMessage = nil
			let conversation = try await queries.conversation(id: id)
			try checkAdmission(revision)
			let pages = HistoryPages(queries: queries) { try await self.checkAdmission(revision) }
			if case .olderPending = action {
				try await loadPending(before: state.pendingBefore, revision: revision)
				apply { $0.loading = false }
				return
			}
			if case .newestPending = action {
				try await loadPending(before: nil, revision: revision)
				apply { $0.loading = false }
				return
			}
			let anchor = readingAnchor(for: action)
			var window = try await historyWindow(action, pages: pages, revision: revision, anchor: anchor)
			if let anchor {
				window = try await recoverVisibleWindow(window, anchor: anchor, pages: pages, revision: revision)
			}
			let target = restorationTarget(for: action, anchor: anchor, window: &window)
			try Task.checkCancellation()
			try checkAdmission(revision)
			if case .refresh = action, target?.changed != true {
				window = cache.refresh(id, page: window, anchor: state.anchorID)
			} else {
				window = cache.put(
					id,
					window: window,
					anchor: anchor?.messageID ?? state.anchorID,
				)
			}
			let preserveNewest = if case .refresh = action {
				state.atNewest && !state.window.rows.isEmpty
			} else {
				false
			}
			publishWindow(window, conversation: conversation, target: target, preserveNewest: preserveNewest)
			try await loadPending(before: nil, revision: revision)
		} catch is CancellationError {} catch {
			guard revision == pendingRevision, !Task.isCancelled else { return }
			apply { $0.loading = false
				$0.error = "Message updates stopped."
			}
		}
	}

	private func checkAdmission(_ revision: UInt64) throws {
		try Task.checkCancellation()
		guard revision == pendingRevision, admit(origin) else { throw CancellationError() }
	}

	private func historyWindow(_ action: ChatTimelineAction, pages: HistoryPages,
	                           revision: UInt64, anchor: ScrollAnchor?) async throws -> HistoryWindow
	{
		if case .older = action {
			let page = try await pages.older(id: id, before: state.window.last)
			try checkAdmission(revision)
			return cache.append(id, page: page, anchor: state.window.rows.last?.id)
		}
		if case .refresh = action, let progress = recovery, progress.newer != nil || progress.older != nil {
			return try await adjacentWindow(progress, pages: pages, revision: revision)
		}
		if let anchor, needsAnchoredWindow(action) {
			recovery = nil
			let window = try await pages.around(id: id, anchor: anchor.position)
			try checkAdmission(revision)
			return window
		}
		if case .refresh = action, !state.window.atNewest || !state.atNewest,
		   let first = state.window.rows.first(where: { $0.id == state.anchorID })?.position ?? state.window.first
		{
			return try await pages.around(id: id, anchor: first)
		}
		return try await pages.older(id: id)
	}

	private func loadPending(before: RecoveryPosition?, revision: UInt64) async throws {
		guard revision == pendingRevision else { throw CancellationError() }
		try Task.checkCancellation()
		var cursor = before
		var skipped: UInt32 = 0
		for _ in 0 ..< 4 {
			let page = try await queries.pending(id: id, before: cursor)
			try Task.checkCancellation()
			guard revision == pendingRevision else { throw CancellationError() }
			skipped += page.skipped
			if !page.rows.isEmpty || !page.hasMore {
				apply {
					$0.error = recoveryMessage
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

	func needsAnchoredWindow(_ action: ChatTimelineAction) -> Bool {
		if case .load = action {
			return true
		}
		return state.window.rows.isEmpty || recovery != nil
	}

	func readingAnchor(for action: ChatTimelineAction) -> ScrollAnchor? {
		if case .load = action {
			return readAnchor()
		}
		if case .refresh = action, let recovery {
			return recovery.anchor
		}
		if case .refresh = action, let id = state.anchorID,
		   let position = state.window.rows.first(where: { $0.id == id })?.position
		{
			return ScrollAnchor(messageID: id, position: position)
		}
		if case .refresh = action, state.window.rows.isEmpty {
			return readAnchor()
		}
		return nil
	}

	func restorationTarget(for action: ChatTimelineAction, anchor: ScrollAnchor?,
	                       window: inout HistoryWindow) -> (id: String?, changed: Bool)?
	{
		guard let anchor else { return nil }
		let target = AnchorRestore.target(anchor, rows: window.rows.filter { $0.content != .reaction })
		if case .refresh = action, !target.changed {
			return nil
		}
		window.changed = target.changed
		if target.changed {
			window.notice = "Position changed"
		}
		return target
	}

	func publishWindow(
		_ window: HistoryWindow,
		conversation: ConversationRecord,
		target: (id: String?, changed: Bool)?,
		preserveNewest: Bool,
	) {
		apply {
			$0.conversation = conversation; $0.window = window
			if let target {
				if let id = target.id {
					$0.anchorID = id
				}
				$0.target = preserveNewest ? nil : target.id
				$0.atNewest = preserveNewest
			}
			$0.loading = false; $0.error = recoveryMessage
		}
	}

	func recoverVisibleWindow(_ initial: HistoryWindow, anchor: ScrollAnchor, pages: HistoryPages,
	                          revision: UInt64) async throws -> HistoryWindow
	{
		var window = initial
		var progress = recovery ?? ReadingRecovery(
			anchor: anchor,
			newer: window.atNewest ? nil : window.first,
			older: window.hasOlder ? window.last : nil,
		)
		for attempt in 0 ..< 2 {
			try checkAdmission(revision)
			if AnchorRestore.target(anchor, rows: window.rows.filter { $0.content != .reaction }).id != nil {
				recovery = nil; return window
			}
			guard progress.newer != nil || progress.older != nil else {
				recovery = progress; recoveryMessage = "No surviving messages are available."; return window
			}
			recovery = progress
			if attempt == 1 {
				recoveryMessage = "More messages are needed to restore your position. Retry to continue."; return window
			}
			window = try await adjacentWindow(progress, pages: pages, revision: revision)
			progress = recovery ?? progress
		}
		return window
	}

	func adjacentWindow(_ progress: ReadingRecovery, pages: HistoryPages, revision: UInt64) async throws -> HistoryWindow {
		try checkAdmission(revision)
		var next = progress
		let window: HistoryWindow
		if let newer = progress.newer {
			window = try await pages.around(id: id, anchor: newer)
			try checkAdmission(revision)
			next.newer = window.atNewest ? nil : window.first
		} else if let older = progress.older {
			window = try await pages.older(id: id, before: older)
			try checkAdmission(revision)
			next.older = window.hasOlder ? window.last : nil
		} else {
			throw ChatError.unavailable
		}
		recovery = next
		return window
	}
}

private struct ReadingRecovery {
	let anchor: ScrollAnchor
	var newer: HistoryPosition?
	var older: HistoryPosition?
}
