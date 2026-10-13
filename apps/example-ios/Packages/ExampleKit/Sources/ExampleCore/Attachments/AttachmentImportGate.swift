import Foundation

/// A transfer keeps its immutable staging context until its importer has completed.
public actor AttachmentImportGate<Context: Sendable> {
	public private(set) var current: Context?
	private var occupied = false
	private var waiting: [CheckedContinuation<Void, Never>] = []
	public init() {}
	public func withContext<Value: Sendable>(_ context: Context,
	                                         operation: @Sendable () async throws -> Value) async throws -> Value
	{
		await acquire()
		defer { current = nil; release() }
		try Task.checkCancellation()
		current = context
		return try await operation()
	}

	private func acquire() async {
		if !occupied {
			occupied = true; return
		}
		await withCheckedContinuation { waiting.append($0) }
	}

	private func release() {
		if waiting.isEmpty {
			occupied = false
		} else {
			waiting.removeFirst().resume()
		}
	}
}
