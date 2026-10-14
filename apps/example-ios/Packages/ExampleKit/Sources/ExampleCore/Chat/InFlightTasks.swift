import Foundation

@MainActor final class InFlightTasks {
	private var tasks: [UUID: Task<Void, Never>] = [:]
	private var closed = false
	var count: Int {
		tasks.count
	}

	@discardableResult func start(_ operation: @escaping @MainActor @Sendable () async -> Void) -> Task<Void, Never>? {
		guard !closed else { return nil }
		let id = UUID()
		let task = Task { @MainActor in
			await operation()
			tasks.removeValue(forKey: id)
		}
		tasks[id] = task
		return task
	}

	func wait() async {
		for task in Array(tasks.values) {
			await task.value
		}
	}

	func close() async {
		closed = true
		let pending = Array(tasks.values)
		pending.forEach { $0.cancel() }
		for task in pending {
			await task.value
		}
	}
}
