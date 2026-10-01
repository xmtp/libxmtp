import Foundation
import XmtpSdk

private actor LogMailbox<T: Sendable> {
    private var values: [T] = []
    private var readers: [CheckedContinuation<T, Never>] = []

    func send(_ value: T) {
        if readers.isEmpty { values.append(value) }
        else { readers.removeFirst().resume(returning: value) }
    }

    func next() async -> T {
        if !values.isEmpty { return values.removeFirst() }
        return await withCheckedContinuation { readers.append($0) }
    }
}

private actor HeldLogSink: LogSink {
    let records = LogMailbox<LogRecord>()
    let responses = LogMailbox<Bool>()
    private(set) var active = 0
    private(set) var maximum = 0

    func log(record: LogRecord) async throws {
        active += 1
        maximum = max(maximum, active)
        defer { active -= 1 }
        await records.send(record)
        if !(await responses.next()) { throw LogSinkError.Failed(reason: "deliberate rejection") }
    }
}

private final class ClosureLogSink: LogSink, @unchecked Sendable {
    let callback: @Sendable (LogRecord) async throws -> Void
    init(_ callback: @escaping @Sendable (LogRecord) async throws -> Void) { self.callback = callback }
    func log(record: LogRecord) async throws { try await callback(record) }
}

private actor LogCompletion {
    var finished = false
    func finish() { finished = true }
}

// verifies: LOG-002, LOG-003, LOG-004, LOG-005, LOG-007, LOG-008, LOG-009
// verifies: LOG-011, LOG-012, LOG-013
func loggingConformance(_ options: ClientOptions) async throws {
    try await initLogging(options: LoggingOptions(level: .error))
    try await loggingSecretsConformance(options)
    let sink = HeldLogSink()
    try await setLogSink(sink: sink)
    try await sdkConformanceEmit(count: 1)
    let first = await sink.records.next()
    guard first.droppedRecords == 0 else { throw ConformanceFailure("first log has drops") }
    try await sdkConformanceEmit(count: 4099)
    await sink.responses.send(true)
    let second = await sink.records.next()
    guard second.fields["sequence"] == "0", second.droppedRecords == 3 else {
        throw ConformanceFailure("log capacity or admission order failed")
    }
    try await sdkConformanceEmit(count: 3)
    await sink.responses.send(false)
    let third = await sink.records.next()
    guard third.fields["sequence"] == "1", third.droppedRecords == 5 else {
        throw ConformanceFailure("failed log lost drops or changed order")
    }
    try await sdkConformanceEmit(count: 2)
    await sink.responses.send(true)
    let fourth = await sink.records.next()
    guard fourth.droppedRecords == 1, await sink.maximum == 1 else {
        throw ConformanceFailure("successful log lost later drops or callbacks overlapped")
    }

    let replaced = LogMailbox<Int>()
    for generation in 1 ... 100 {
        try await setLogSink(sink: nil)
        try await setLogSink(sink: ClosureLogSink { _ in
            guard await sink.active == 0 else { throw ConformanceFailure("replacement overlapped old callback") }
            await replaced.send(generation)
        })
        try await sdkConformanceEmit(count: 1)
    }
    guard await sink.active == 1 else { throw ConformanceFailure("replacement cancelled the active callback") }
    await sink.responses.send(false)
    guard await replaced.next() == 100 else { throw ConformanceFailure("replaced generation was handed off") }
    try await setLogSink(sink: nil)

    let completion = LogCompletion()
    let next = LogMailbox<Bool>()
    try await setLogSink(sink: ClosureLogSink { _ in
        try await setLogSink(sink: ClosureLogSink { _ in
            await next.send(await completion.finished)
        })
        try await sdkConformanceEmit(count: 1)
        await completion.finish()
    })
    try await sdkConformanceEmit(count: 1)
    guard await next.next() else { throw ConformanceFailure("reentrant replacement overlapped callbacks") }
    try await setLogSink(sink: nil)
    var endOptions = options
    endOptions.storage = StorageOptions(location: .inMemory)
    endOptions.registration = RegistrationOptions(auto: false)
    let client = try await SDKClient.create(signer: await generateLocalSigner(), options: endOptions)
    try await loggingEndConformance(client)
    print("Swift logging: async queue, overflow, failure, generations and reentry passed")
}

// verifies: LOG-008
func loggingEndConformance(_ client: SDKClient) async throws {
    try await initLogging(options: LoggingOptions(level: .error))
    let ended = LogMailbox<Bool>()
    try await setLogSink(sink: ClosureLogSink { _ in
        do {
            try await client.end()
            try await setLogSink(sink: nil)
            await ended.send(true)
        } catch { await ended.send(false) }
    })
    try await sdkConformanceEmit(count: 1)
    guard await ended.next() else { throw ConformanceFailure("client end from log callback failed") }
    do {
        _ = try await client.isRegistered()
        throw ConformanceFailure("log callback left the client open")
    } catch XmtpError.ClientClosed {}
}

// verifies: LOG-010
private func loggingSecretsConformance(_ options: ClientOptions) async throws {
    let credential = "LOG_CREDENTIAL_SENTINEL_89d42"
    let signing = Data("LOG_SIGNING_KEY_SENTINEL_89d42!!!".utf8)
    let database = Data("LOG_DATABASE_KEY_SENTINEL_89d42".utf8)
    let forbidden = [credential] + [signing, database].flatMap { bytes in
        [String(decoding: bytes, as: UTF8.self), bytes.map { String(format: "%02x", $0) }.joined(), String(describing: Array(bytes))]
    }
    let observed = LogMailbox<(String, Bool)>()
    try await setLogSink(sink: ClosureLogSink { record in
        await observed.send((([record.message] + Array(record.fields.values)).joined(separator: "\n"), record.target == "xmtp_sdk::conformance"))
    })
    do {
        _ = try await Backend.connect(options: BackendOptions(url: "http://127.0.0.1:1", credential: Credential(name: nil, value: "Bearer \(credential)\n", expiresAtSeconds: 0)))
        throw ConformanceFailure("invalid credential was accepted")
    } catch XmtpError.InvalidInput {}
    do {
        _ = try await localSignerFromPrivateKey(key: signing)
        throw ConformanceFailure("invalid signing key was accepted")
    } catch XmtpError.InvalidInput {}
    var invalidStorage = options
    invalidStorage.storage = StorageOptions(location: .inMemory, encryptionKey: database)
    do {
        _ = try await SDKClient.create(signer: await generateLocalSigner(), options: invalidStorage)
        throw ConformanceFailure("invalid encryption key was accepted")
    } catch is XmtpError {}
    try await sdkConformanceEmit(count: 1)
    while true {
        let (text, last) = await observed.next()
        guard !forbidden.contains(where: text.contains) else { throw ConformanceFailure("app log exposed a secret") }
        if last { break }
    }
    try await setLogSink(sink: nil)
}
