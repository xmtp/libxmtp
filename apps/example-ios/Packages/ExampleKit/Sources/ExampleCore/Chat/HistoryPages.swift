import Foundation

public struct HistoryWindow: Equatable, Sendable {
	public var rows: [ChatMessage] =
		[]
	public var first: HistoryPosition?
	public var last: HistoryPosition?
	public var hasOlder =
		false
	public var atNewest = true
	public var notice: String?
	public var changed = false
	public init() {}
}

public struct HistoryPages: Sendable {
	private let queries: any ChatQueries
	private let admitted: @Sendable () async throws -> Void
	public init(queries: any ChatQueries) {
		self.queries = queries
		admitted = { try Task.checkCancellation() }
	}

	init(queries: any ChatQueries, admitted: @escaping @Sendable () async throws -> Void) {
		self.queries = queries
		self.admitted = admitted
	}

	private func readable(
		id: String,
		ascending: Bool,
		before: HistoryPosition?,
		after: HistoryPosition?,
		budget: Int,
	) async throws -> (ChatPage, Int) {
		var upper = before
		var lower = after
		var first: HistoryPosition?
		var skipped: UInt32 = 0
		var count = 0
		while true {
			try Task.checkCancellation()
			try await admitted()
			let page = try await queries
				.history(id: id, ascending: ascending, before: upper, after: lower)
			try Task.checkCancellation()
			try await admitted()
			count += 1
			first = first ?? page
				.first
			skipped += page.skipped
			if !page.rows.isEmpty || !page.hasMore || count >= budget {
				return (
					ChatPage(rows: page.rows, first: first, last: page.last, hasMore: page.hasMore, skipped: skipped),
					count,
				)
			}
			guard let next = page.last else { throw ChatError.unavailable }
			if ascending {
				lower = next
			} else {
				upper = next
			}
		}
	}

	public func older(id: String, before: HistoryPosition? = nil) async throws -> HistoryWindow {
		let (page, _) = try await readable(id: id, ascending: false, before: before, after: nil, budget: 4)
		var window = HistoryWindow()
		window.rows = page.rows
		window.first = page.first
		window.last = page.last
		window
			.hasOlder = page.hasMore
		window.atNewest = before == nil
		if page.skipped > 0 {
			window.notice = "Some stored history cannot be read. Load more to continue."
		}
		return window
	}

	public func around(id: String, anchor: HistoryPosition) async throws -> HistoryWindow {
		let (newer, count) = try await readable(id: id, ascending: true, before: nil, after: anchor, budget: 2)
		let (older, _) = try await readable(id: id, ascending: false, before: newer.first, after: nil, budget: 4 - count)
		var window = HistoryWindow()
		window.rows = newer.rows.reversed() + older.rows
		window.first = newer.last ?? older
			.first
		window.last = older.last
		window.hasOlder = older.hasMore
		window.atNewest = !newer.hasMore
		if newer.skipped + older
			.skipped > 0
		{
			window.notice = "Some stored history cannot be read. Load more to continue."
		}
		return window
	}
}
