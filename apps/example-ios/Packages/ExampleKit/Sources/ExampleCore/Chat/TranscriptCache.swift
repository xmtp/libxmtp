import Foundation

@MainActor public final class TranscriptCache {
	private var values: [String: HistoryWindow] = [:]
	private var order: [String] = []
	public init() {}
	public func get(_ id: String) -> HistoryWindow? {
		guard let value = values[id]
		else { return nil }
		touch(id)
		return value
	}

	public func clear() {
		values.removeAll()
		order.removeAll()
	}

	public func put(_ id: String, window: HistoryWindow, anchor: String? = nil) -> HistoryWindow {
		var result = window
		var seen: Set<String> = []
		let rows = window.rows.filter { seen.insert($0.id).inserted }
		let index = rows.firstIndex { $0.id == anchor } ?? 0
		let start = max(0, min(index - 250, rows.count - 500))
		result
			.rows = Array(rows.dropFirst(start).prefix(500))
		if start > 0 {
			result.first = result.rows.first?.position
			result.atNewest = false
		}
		if start + result.rows.count < rows.count {
			result.last = result.rows.last?.position
			result.hasOlder = true
		}
		values[id] = result
		touch(id)
		while order
			.count > 3
		{
			values.removeValue(forKey: order.removeFirst())
		}
		return result
	}

	public func append(_ id: String, page: HistoryWindow,
	                   anchor: String?)
		-> HistoryWindow
	{
		var next = page
		if let old = values[id] {
			next.rows = old.rows + page.rows
			next.first = old.first
			next.atNewest = old.atNewest
			next.notice = old
				.notice ?? page.notice
		}
		return put(
			id,
			window: next,
			anchor: anchor,
		)
	}

	public func refresh(_ id: String, page: HistoryWindow, anchor: String?) -> HistoryWindow {
		guard let previous = values[id] else { return put(id, window: page, anchor: anchor) }
		let refreshed = Set(page.rows.map(\.id))
		guard let first = previous.rows.firstIndex(where: { refreshed.contains($0.id) }),
		      let last = previous.rows.lastIndex(where: { refreshed.contains($0.id) })
		else { return previous.atNewest && page.atNewest ? put(
			id,
			window: page,
			anchor: nil,
		) : previous }
		let prefix = Array(previous.rows.prefix(first))
		let suffix = Array(previous.rows.dropFirst(last + 1))
		var next = page
		var replacement = page.rows
		let previousIDs = Set(previous.rows.map(\.id))
		if !previous.atNewest,
		   let start = replacement
		   .firstIndex(where: { previousIDs.contains($0.id) })
		{
			replacement = Array(replacement.dropFirst(start))
		}
		if previous.hasOlder,
		   let end = replacement
		   .lastIndex(where: { previousIDs.contains($0.id) })
		{
			replacement = Array(replacement.prefix(end + 1))
		}
		next.rows = prefix + replacement + suffix
		next.first = prefix.isEmpty && previous.atNewest ? page.first : previous.first
		next.last = suffix.isEmpty && !previous.hasOlder ? page.last : previous.last
		next.hasOlder = suffix.isEmpty ? previous.hasOlder || page.hasOlder : previous.hasOlder
		next.atNewest = previous.atNewest && (!prefix.isEmpty || page.atNewest)
		next.notice = page.notice ?? previous.notice
		return put(id, window: next, anchor: anchor)
	}

	private func touch(_ id: String) {
		order.removeAll { $0 == id }
		order.append(id)
	}
}
