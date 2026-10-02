import Foundation

private final class CallerTaskResult<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var completed: Result<Value, Error>?

    func finish(_ value: Result<Value, Error>) {
        lock.lock()
        completed = value
        lock.unlock()
    }

    func result() -> Result<Value, Error>? {
        lock.lock()
        defer { lock.unlock() }
        return completed
    }
}

func callerResult<Value>(_ call: Task<Value, Error>, _ label: String) async throws -> Result<Value, Error> {
    let result = CallerTaskResult<Value>()
    Task { result.finish(await call.result) }
    do {
        try await lifetimeWait(label) { result.result() != nil }
    } catch {
        call.cancel()
        throw error
    }
    return result.result()!
}

func callerValue<Value>(_ call: Task<Value, Error>, _ label: String) async throws -> Value {
    try await callerResult(call, label).get()
}
