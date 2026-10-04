import Foundation
@testable import XmtpSdk

private func eventCancellationCase(_ backend: BackendOptions, _ mode: String, _ usesIterator: Bool) async throws {
    var options = lifetimeOptions(backend)
    let deletion = mode.contains("Delete")
    let missingFile = mode.hasSuffix("DeleteMissing")
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent("sdk-event-delete-" + UUID().uuidString)
    defer {
        if deletion {
            try? FileManager.default.removeItem(at: directory)
        }
    }
    if deletion {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        options.storage = StorageOptions(location: .explicit(dbPath: directory.appendingPathComponent("xmtp.db3").path, attachmentsDir: directory.appendingPathComponent("attachments").path), singleConnection: true)
    }
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
    let filter = EventFilter(kinds: [.hmacKeysUpdated], groupIds: nil, contentTypes: nil, referencesOwnMessages: false)
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
        if mode == "prepoll" {
            await start.wait()
        }
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
                if missingFile {
                    try FileManager.default.removeItem(atPath: path)
                }
                do {
                    try await storage.delete()
                    if missingFile {
                        throw ConformanceFailure("missing file deletion did not fail")
                    }
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
        if mode != "clientEndReady" {
            try await client.end()
        }
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
    let filter = EventFilter(kinds: [.hmacKeysUpdated], groupIds: nil, contentTypes: nil, referencesOwnMessages: false)
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

private final class EventCloseCompletion: @unchecked Sendable {
    private let lock = NSLock()
    private var complete = false
    func finish() {
        lock.lock(); complete = true; lock.unlock()
    }

    func isComplete() -> Bool {
        lock.lock(); defer { lock.unlock() }; return complete
    }
}

private func eventCloseCancellation(_ backend: BackendOptions, _ deletion: Bool, _ shutdown: Bool, _ missingFile: Bool = false, _ beforeStart: Bool = false) async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent("sdk-event-close-" + UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let path = directory.appendingPathComponent("xmtp.db3").path
    var options = lifetimeOptions(backend)
    options.storage = StorageOptions(location: .explicit(dbPath: path, attachmentsDir: directory.appendingPathComponent("attachments").path), singleConnection: true)
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
    let probe = await SdkConformanceConstructorProbe.open()
    probe.observeClient(client: client.raw)
    let filter = EventFilter(kinds: [.hmacKeysUpdated], groupIds: nil, contentTypes: nil, referencesOwnMessages: false)
    let raw = try await client.raw.events(filter: filter)
    let iterator = try await client.events(filter).makeAsyncIterator()
    await sdkConformanceSwiftEventGate.release()
    await sdkConformanceSwiftEventLiftGate.release()
    await sdkConformanceSwiftEventFinalGate.release()
    await sdkConformanceSwiftClientEndGate.release()
    await sdkConformanceSwiftStorageDeleteGate.release()
    sdkConformanceSwiftEventCalls.reset()
    let rawRead = Task { try await raw.next() }
    let iteratorRead = Task { try await iterator.next() }
    let admission = deletion ? sdkConformanceSwiftStorageDeleteGate : sdkConformanceSwiftClientEndGate
    let counts = deletion ? sdkConformanceSwiftStorageDeleteCalls : sdkConformanceSwiftClientEndCalls
    let completion = EventCloseCompletion()
    let start = LifetimeGate()
    var close: Task<Void, Error>?
    do {
        try await lifetimeWait("close two pending native reads") { sdkConformanceSwiftEventCalls.snapshot()["poll", default: 0] >= 2 }
        if missingFile {
            try FileManager.default.removeItem(atPath: path)
        }
        if shutdown {
            probe.holdShutdown()
        } else if !beforeStart {
            await admission.reset()
        }
        counts.reset()
        let call = Task {
            defer { completion.finish() }
            if beforeStart {
                await start.wait()
            }
            if deletion {
                try await client.raw.storage().delete()
            } else {
                try await client.end()
            }
        }
        close = call
        if beforeStart {
            call.cancel()
            await start.release()
        } else {
            if shutdown {
                try await lifetimeWait("native shutdown hold") { probe.shutdownEntered() }
                let state = probe.state()
                guard state.storeConnected, !state.workersStopped else { throw ConformanceFailure("shutdown gate did not hold native close") }
            } else {
                let deadline = ContinuousClock.now + .seconds(20)
                while !(await admission.didEnter()) {
                    guard ContinuousClock.now < deadline else { throw ConformanceFailure("close admission gate timed out") }
                    try await Task.sleep(for: .milliseconds(1))
                }
            }
            call.cancel()
            if shutdown {
                for _ in 0 ..< 20 {
                    await Task.yield()
                }
                guard !completion.isComplete() else { throw ConformanceFailure("cancelled caller returned while native shutdown was held") }
                probe.releaseShutdown()
            } else {
                await admission.release()
            }
        }
        switch try await callerResult(call, "cancelled native close") {
        case .success: throw ConformanceFailure("close lost original caller cancellation")
        case let .failure(error):
            if missingFile {
                guard let nativeError = error as? XmtpError,
                      case let .Storage(details) = nativeError,
                      details.code == "Storage", details.category == .storage
                else { throw error }
            } else {
                guard error is CancellationError else { throw error }
            }
        }
        let state = probe.state()
        guard state.clientClosed, state.workersStopped, !state.storeConnected else {
            throw ConformanceFailure("cancelled close stopped Swift Events but left native client or database open")
        }
        let native = counts.snapshot()
        guard native["complete"] == 1, native["free"] == 1, native["cancel", default: 0] == 0 else {
            throw ConformanceFailure("owned close future was cancelled or not completed once: \(native)")
        }
        guard try await callerValue(rawRead, "raw read after cancelled close") == nil else { throw ConformanceFailure("raw event after cancelled close") }
        guard try await callerValue(iteratorRead, "iterator after cancelled close") == nil else { throw ConformanceFailure("iterator event after cancelled close") }
        guard FileManager.default.fileExists(atPath: path) == !deletion else { throw ConformanceFailure("cancelled close changed wrong file state") }
        print("Swift owned close: deletion=\(deletion), nativeShutdown=\(shutdown), beforeStart=\(beforeStart), missingFile=\(missingFile), native=\(native), disconnected=true, raw=nil, iterator=nil")
        try await probe.cleanup()
    } catch {
        close?.cancel()
        await start.release()
        await admission.release()
        probe.releaseShutdown()
        _ = try? await Task.detached { try await raw.end(); try await client.end(); try await probe.cleanup() }.value
        _ = try? await callerResult(rawRead, "failed close raw read")
        _ = try? await callerResult(iteratorRead, "failed close iterator read")
        throw error
    }
}

private func eventDeleteBeforeAdmission(_ backend: BackendOptions) async throws {
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: lifetimeOptions(backend))
    let start = LifetimeGate()
    let call = Task { await start.wait(); try await client.raw.storage().delete() }
    call.cancel()
    await start.release()
    do { try await callerValue(call, "delete before path cancellation"); throw ConformanceFailure("delete before admission returned") }
    catch is CancellationError {}
    _ = try client.raw.sdkConformanceEmitHmacEvents(count: 0)
    let filter = EventFilter(kinds: [.hmacKeysUpdated], groupIds: nil, contentTypes: nil, referencesOwnMessages: false)
    let reader = try await client.raw.events(filter: filter)
    guard !reader.sdkEventReadGate.isEnded() else { throw ConformanceFailure("pre-admission cancellation stopped Event gates") }
    try await reader.end()
    try await client.end()
    print("Swift deletion cancellation before admission leaves native client and Events live")
}

func checkSwiftEventCancellation(_ backend: BackendOptions) async throws {
    try await eventDeleteBeforeAdmission(backend)
    for deletion in [false, true] {
        for shutdown in [false, true] {
            try await eventCloseCancellation(backend, deletion, shutdown)
        }
    }
    try await eventCloseCancellation(backend, true, true, true)
    try await eventCloseCancellation(backend, false, false, false, true)
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
