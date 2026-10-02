import Foundation
@testable import XmtpSdk

private func eventCancellationCase(_ backend: BackendOptions, _ mode: String, _ usesIterator: Bool) async throws {
    var options = lifetimeOptions(backend)
    let deletion = mode.contains("Delete")
    let missingFile = mode.hasSuffix("DeleteMissing")
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent("sdk-event-delete-" + UUID().uuidString)
    defer { if deletion { try? FileManager.default.removeItem(at: directory) } }
    if deletion {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        options.storage = StorageOptions(location: .explicit(dbPath: directory.appendingPathComponent("xmtp.db3").path, attachmentsDir: directory.appendingPathComponent("attachments").path), singleConnection: true)
    }
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
    let filter = EventFilter(kinds: [.hmacKeysUpdated], conversationIds: nil, contentTypes: nil, referencesOwnMessages: false)
    let reader = try await client.raw.events(filter: filter)
    let iterator = try await client.events(filter).makeAsyncIterator()
    await sdkConformanceSwiftEventGate.release()
    await sdkConformanceSwiftEventLiftGate.release()
    await sdkConformanceSwiftEventFinalGate.release()
    let gate = mode == "finalCancel" ? sdkConformanceSwiftEventFinalGate : mode.hasPrefix("lift") ? sdkConformanceSwiftEventLiftGate : sdkConformanceSwiftEventGate
    let gated = mode != "prepoll" && mode != "pending"
    if gated {
        await gate.reset()
        _ = try client.raw.sdkConformanceEmitHmacEvents(count: 1)
    }
    sdkConformanceSwiftEventCalls.reset()
    sdkConformanceSwiftEventEndCalls.reset()
    let start = LifetimeGate()
    let call = Task {
        if mode == "prepoll" { await start.wait() }
        return try await (usesIterator ? iterator.next() : reader.next())
    }
    do {
        if mode == "prepoll" {
            call.cancel()
            await start.release()
        } else if mode == "pending" {
            try await lifetimeWait("pending event poll") { sdkConformanceSwiftEventCalls.snapshot()["poll", default: 0] > 0 }
            call.cancel()
        } else {
            let deadline = ContinuousClock.now + .seconds(20)
            while !(await gate.didEnter()) {
                guard ContinuousClock.now < deadline else { throw ConformanceFailure("event gate timed out") }
                try await Task.sleep(for: .milliseconds(1))
            }
            if deletion {
                let storage = client.raw.storage()
                guard let path = try await storage.path() else { throw ConformanceFailure("delete fixture has no path") }
                if missingFile { try FileManager.default.removeItem(atPath: path) }
                do {
                    try await storage.delete()
                    if missingFile { throw ConformanceFailure("missing file deletion did not fail") }
                } catch {
                    guard missingFile, error is XmtpError else { throw error }
                    print("Swift file deletion error after native close: \(error)")
                }
                guard !FileManager.default.fileExists(atPath: path) else { throw ConformanceFailure("deleted database remains") }
            } else if mode == "clientEndReady" {
                try await client.end()
            } else if mode.hasSuffix("End") {
                try await reader.end()
            } else {
                call.cancel()
            }
            await gate.release()
        }
        let value = try await callerValue(call, "ended event result")
        guard value == nil else { throw ConformanceFailure("event handed off after end or cancellation: \(mode)") }
        let counts = sdkConformanceSwiftEventCalls.snapshot()
        guard counts["free"] == 1 else { throw ConformanceFailure("event future was not freed once: \(counts)") }
        if mode.hasPrefix("lift") || mode == "finalCancel" {
            guard counts["complete"] == 1, counts["cancel", default: 0] == 0 else {
                throw ConformanceFailure("event native cancellation ran after completion: \(counts)")
            }
        }
        let endCounts = sdkConformanceSwiftEventEndCalls.snapshot()
        guard deletion || mode == "clientEndReady" || (endCounts["complete", default: 0] > 0 && endCounts["free", default: 0] > 0) else {
            throw ConformanceFailure("event cancellation did not finish native end: \(endCounts)")
        }
        if usesIterator {
            guard try await iterator.next() == nil else { throw ConformanceFailure("ended event iterator reopened") }
        } else {
            guard try await reader.next() == nil else { throw ConformanceFailure("ended event reader reopened") }
        }
        print("Swift event ended: mode=\(mode), iterator=\(usesIterator), native=\(counts), nativeEnd=\(endCounts), value=nil")
        try await reader.end()
        if mode != "clientEndReady" { try await client.end() }
    } catch {
        call.cancel()
        await start.release()
        await sdkConformanceSwiftEventGate.release()
        await sdkConformanceSwiftEventLiftGate.release()
        await sdkConformanceSwiftEventFinalGate.release()
        _ = try? await callerResult(call, "failed event result")
        try? await client.end()
        throw error
    }
}

private func eventMemoryDeleteRejected(_ backend: BackendOptions) async throws {
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: lifetimeOptions(backend))
    let filter = EventFilter(kinds: [.hmacKeysUpdated], conversationIds: nil, contentTypes: nil, referencesOwnMessages: false)
    let reader = try await client.raw.events(filter: filter)
    do {
        do {
            try await client.raw.storage().delete()
            throw ConformanceFailure("in-memory deletion did not fail")
        } catch {
            guard error is XmtpError else { throw error }
        }
        _ = try client.raw.sdkConformanceEmitHmacEvents(count: 1)
        let call = Task { try await reader.next() }
        guard try await callerValue(call, "event after invalid delete") != nil else {
            throw ConformanceFailure("in-memory deletion stopped Event handoff")
        }
        try await reader.end()
        try await client.end()
        print("Swift in-memory delete rejected; Event handoff remains live")
    } catch {
        try? await client.end()
        throw error
    }
}

func checkSwiftEventCancellation(_ backend: BackendOptions) async throws {
    try await eventMemoryDeleteRejected(backend)
    for usesIterator in [false, true] {
        let modes = usesIterator
            ? ["prepoll", "pending", "readyCancel", "liftCancel", "finalCancel", "readyDelete", "liftDelete", "readyDeleteMissing", "liftDeleteMissing", "clientEndReady"]
            : ["prepoll", "pending", "readyCancel", "liftCancel", "finalCancel", "readyDelete", "liftDelete", "readyDeleteMissing", "liftDeleteMissing", "readyEnd", "liftEnd", "clientEndReady"]
        for mode in modes {
            try await eventCancellationCase(backend, mode, usesIterator)
        }
    }
}
