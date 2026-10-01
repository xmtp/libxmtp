import Foundation
@testable import XmtpSdk

private actor CallerStartGate {
    var open = false
    var waiting: [CheckedContinuation<Void, Never>] = []
    func wait() async {
        if open {
            return
        }; await withCheckedContinuation { waiting.append($0) }
    }

    func release() {
        open = true; for c in waiting {
            c.resume()
        }; waiting.removeAll()
    }
}

private func callerCancellationCase(_ backend: BackendOptions, _ mode: String) async throws {
    if mode == "prepoll" || mode == "pending" {
        let url = backend.url
        let signer = await generateLocalSigner()
        let options = ClientOptions(backend: .options(options: BackendOptions(url: url)), storage: StorageOptions(location: .inMemory), deviceSync: false)
        let client = try await SDKClient.create(signer: signer, options: options)
        let group = try await client.conversations().createGroup(members: [InboxId](), options: CreateGroupOptions(name: "poll boundary proof"))
        let reader = try await group.messageReader()
        await sdkConformanceSwiftReaderGate.release()
        if mode == "prepoll" {
            let first = try await group.send(encoded: TextCodec().encode("prepoll first"), options: nil)
            let delivered = try await reader.next()
            guard delivered?.id == first else { throw ConformanceFailure("wrong first item") }
            sdkConformanceSwiftReaderCalls.reset()
            let gate = CallerStartGate()
            let call = Task { await gate.wait(); return try await reader.next() }
            call.cancel()
            await gate.release()
            do { _ = try await call.value; throw ConformanceFailure("prepoll cancellation returned") }
            catch is CancellationError {}
            let counts = sdkConformanceSwiftReaderCalls.snapshot()
            guard counts == ["cancel": 1, "free": 1] else { throw ConformanceFailure("prepoll native counts \(counts)") }
            print("prepoll native counts \(counts)")
            try await reader.end()
            let replay = try await group.messageReader()
            let persisted = try await replay.next()
            guard persisted?.id == first else { throw ConformanceFailure("prepoll cancellation acknowledged unseen next read") }
            print("prepoll cancelled read did not ACK prior item; it replays")
            try await replay.end()
        } else {
            sdkConformanceSwiftReaderCalls.reset()
            let call = Task { try await reader.next() }
            let deadline = ContinuousClock.now + .seconds(20)
            while sdkConformanceSwiftReaderCalls.snapshot()["poll", default: 0] == 0 {
                guard ContinuousClock.now < deadline else { throw ConformanceFailure("pending first poll timed out") }
                try await Task.sleep(for: .milliseconds(1))
            }
            call.cancel()
            do { _ = try await call.value; throw ConformanceFailure("pending cancellation returned") }
            catch is CancellationError {}
            let counts = sdkConformanceSwiftReaderCalls.snapshot()
            guard counts["cancel"] == 1, counts["free"] == 1, counts["complete"] == 1 else { throw ConformanceFailure("pending native counts \(counts)") }
            print("pending native counts \(counts)")
            print("pending real native future cancelled and freed once")
        }
        try await reader.end()
        try await client.end()
        return
    }
    if mode == "reader" || mode == "lift" {
        let url = backend.url
        let signer = await generateLocalSigner()
        let options = ClientOptions(backend: .options(options: BackendOptions(url: url)), storage: StorageOptions(location: .inMemory), deviceSync: false)
        let client = try await SDKClient.create(signer: signer, options: options)
        let group = try await client.conversations().createGroup(members: [InboxId](), options: CreateGroupOptions(name: "ready reader proof"))
        let first = try await group.send(encoded: TextCodec().encode("first ready item"), options: nil)
        let second = try await group.send(encoded: TextCodec().encode("second ready item"), options: nil)
        let reader = try await group.messageReader()
        await sdkConformanceSwiftReaderGate.release()
        await sdkConformanceSwiftReaderLiftGate.release()
        let gate = mode == "lift" ? sdkConformanceSwiftReaderLiftGate : sdkConformanceSwiftReaderGate
        await gate.reset()
        sdkConformanceSwiftReaderCalls.reset()
        let call = Task { try await reader.next() }
        let deadline = ContinuousClock.now + .seconds(20)
        while !(await gate.didEnter()) {
            guard ContinuousClock.now < deadline else { throw ConformanceFailure("reader READY gate timed out") }
            try await Task.sleep(for: .milliseconds(1))
        }
        call.cancel()
        await gate.release()
        switch await call.result {
        case let .success(item):
            guard item?.id == first else { throw ConformanceFailure("wrong ready item") }
            if mode == "lift" {
                let counts = sdkConformanceSwiftReaderCalls.snapshot()
                guard counts["complete"] == 1, counts["cancel", default: 0] == 0, counts["free"] == 1 else { throw ConformanceFailure("native cancellation ran after completion: \(counts)") }
                print("post-complete lift native counts \(counts)")
            }
            print("READY delivered first item despite late cancellation")
            try await reader.end()
            let replay = try await group.messageReader()
            let persisted = try await replay.next()
            guard persisted?.id == first else { throw ConformanceFailure("returned ready item was acknowledged by close") }
            print("READY first item remains unacknowledged on close and replays")
            try await replay.end()
        case let .failure(error):
            print("READY item not delivered; error=\(error)")
            let retry = try await reader.next()
            print("retry returned second=\(retry?.id == second), first=\(retry?.id == first)")
            try await reader.end()
            let replay = try await group.messageReader()
            let persisted = try await replay.next()
            print("reopen returned second=\(persisted?.id == second), first=\(persisted?.id == first)")
            try await replay.end()
            guard retry?.id == second, persisted?.id == second else { throw ConformanceFailure("expected ack loss not reproduced") }
            throw ConformanceFailure("first item was never delivered but retry durably acknowledged it")
        }
        try await reader.end()
        try await client.end()
        return
    }
    let gate = CallerStartGate()
    let task = Task { await gate.wait(); _ = await generateLocalSigner(); print("NONTHROWING RETURNED") }
    task.cancel()
    await gate.release()
    await task.value
}

func checkSwiftCallerCancellation(backend: BackendOptions) async throws {
    for mode in ["nonthrowing", "prepoll", "pending", "reader", "lift"] {
        try await callerCancellationCase(backend, mode)
    }
}
