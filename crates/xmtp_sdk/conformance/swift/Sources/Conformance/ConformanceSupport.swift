import Foundation
@testable import XmtpSdk

struct ConformanceFailure: LocalizedError {
    let errorDescription: String?

    init(_ check: String) {
        errorDescription = check
    }
}

final class TestFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var open = false

    func set() {
        lock.lock()
        open = true
        lock.unlock()
    }

    var value: Bool {
        lock.lock()
        defer { lock.unlock() }
        return open
    }
}

final class TestCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    func increment() {
        lock.lock()
        count += 1
        lock.unlock()
    }

    var value: Int {
        lock.lock()
        defer { lock.unlock() }
        return count
    }
}

actor EventSignal {
    private var seen = false

    func mark() {
        seen = true
    }

    func hasRun() -> Bool {
        seen
    }
}

actor EventStartPause {
    private var entered = false
    private var released = false

    func hold() async {
        entered = true
        while !released {
            do {
                try await Task.sleep(nanoseconds: 10_000_000)
            } catch {
                return
            }
        }
    }

    func waitUntilEntered() async throws {
        for _ in 0 ..< 1000 {
            if entered {
                return
            }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        throw ConformanceFailure("event listener start hook did not run")
    }

    func release() {
        released = true
    }
}
