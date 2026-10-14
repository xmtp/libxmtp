import Foundation
import Observation

/// Decoding reservations and retained previews share one three-image memory budget.
@MainActor @Observable public final class PreviewCache<Value> {
	public private(set) var values: [String: Value] = [:]
	private var order: [String] = []
	private var decoding = 0
	private var generation: UInt64 = 0
	private var waiters: [CheckedContinuation<Void, Never>] = []
	var waitingCount: Int {
		waiters.count
	}

	public init() {}
	public func load(id: String, decode: () async -> Value?) async {
		if values[id] != nil {
			return
		}
		let origin = generation
		while values.count + decoding >= 3 {
			if !order.isEmpty {
				values.removeValue(forKey: order.removeFirst())
			} else {
				await withCheckedContinuation { waiters.append($0) }
			}
			if Task.isCancelled || origin != generation {
				resumeNext()
				return
			}
		}
		decoding += 1
		let value = await decode()
		decoding -= 1
		if !Task.isCancelled, origin == generation, let value {
			values[id] = value
			order.removeAll { $0 == id }
			order.append(id)
		}
		resumeNext()
	}

	private func resumeNext() {
		if !waiters.isEmpty {
			waiters.removeFirst().resume()
		}
	}

	public func clear() {
		generation += 1
		values.removeAll()
		order.removeAll()
		let pending = waiters
		waiters.removeAll()
		pending.forEach { $0.resume() }
	}
}
