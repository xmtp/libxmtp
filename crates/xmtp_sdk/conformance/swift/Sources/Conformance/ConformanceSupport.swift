import Foundation
@testable import XmtpSdk

struct ConformanceFailure: LocalizedError {
    let errorDescription: String?

    init(_ check: String) {
        errorDescription = check
    }
}

func sameEncoded(_ lhs: EncodedContent, _ rhs: EncodedContent) -> Bool {
    lhs.type.authorityId == rhs.type.authorityId &&
        lhs.type.typeId == rhs.type.typeId &&
        lhs.type.versionMajor == rhs.type.versionMajor &&
        lhs.type.versionMinor == rhs.type.versionMinor &&
        lhs.parameters == rhs.parameters &&
        lhs.fallback == rhs.fallback &&
        lhs.content == rhs.content
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

final class TestSigner: Signer, @unchecked Sendable {
    private func run(_ action: String, _ text: String? = nil) throws -> String {
        let environment = ProcessInfo.processInfo.environment
        let process = Process()
        process.executableURL = URL(fileURLWithPath: environment["SDK_NODE_BIN"]!)
        process.arguments = [environment["SDK_SIGN_SCRIPT"]!, action] + (text.map { [$0] } ?? [])
        let output = Pipe()
        process.standardOutput = output
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw ConformanceFailure("sign command failed") }
        return String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)!
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func identity() async throws -> PublicIdentity {
        try PublicIdentity(identifier: run("identity"), kind: .ethereum)
    }

    func kind() async throws -> SignerKind {
        .eoa
    }

    func sign(request: SigningRequest) async throws -> Signature {
        let hex = try String(run("sign", request.text).dropFirst(2))
        let bytes = stride(from: 0, to: hex.count, by: 2).map { offset -> UInt8 in
            let start = hex.index(hex.startIndex, offsetBy: offset)
            let end = hex.index(start, offsetBy: 2)
            return UInt8(hex[start ..< end], radix: 16)!
        }
        return .ecdsa(Data(bytes))
    }
}

final class CallLog: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String] = []

    func append(_ value: String) {
        lock.lock()
        values.append(value)
        lock.unlock()
    }

    func removeAll() {
        lock.lock()
        values.removeAll()
        lock.unlock()
    }

    var calls: [String] {
        lock.lock()
        defer { lock.unlock() }
        return values
    }
}

final class RecordingSigner: Signer, @unchecked Sendable {
    private let inner: Signer
    private let log: CallLog

    init(_ inner: Signer, _ log: CallLog) {
        self.inner = inner
        self.log = log
    }

    func identity() async throws -> PublicIdentity {
        try await inner.identity()
    }

    func kind() async throws -> SignerKind {
        try await inner.kind()
    }

    func sign(request: SigningRequest) async throws -> Signature {
        log.append("sign")
        return try await inner.sign(request: request)
    }
}

final class RecordingPreAuthenticate: PreAuthenticate, @unchecked Sendable {
    private let log: CallLog
    private let fail: Bool

    init(_ log: CallLog, fail: Bool) {
        self.log = log
        self.fail = fail
    }

    func run() async throws {
        log.append("pre-authenticate")
        if fail {
            throw PreAuthenticateError.Failed
        }
    }
}

final class OrderedLogSink: LogSink, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String] = []

    func log(record: LogRecord) throws {
        guard record.target == "xmtp_sdk::conformance" else { return }
        lock.lock()
        values.append(record.fields["sequence"] ?? "")
        lock.unlock()
    }

    func sequence() -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return values
    }
}

actor EventSignal {
    private var seen = false

    func mark() {
        seen = true
    }

    func wait() async throws {
        for _ in 0 ..< 100 {
            if seen {
                return
            }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        throw ConformanceFailure("event listener did not run")
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

struct SampleCodec: SDKContentCodec {
    let type = ContentTypeId(authorityId: "example.org", typeId: "sample", versionMajor: 1, versionMinor: 0)
    func encode(_ value: any Sendable) throws -> EncodedContent {
        guard let text = value as? String else { throw ConformanceFailure("custom value was not text") }
        return EncodedContent(type: type, content: Data(text.utf8))
    }

    func decode(_ encoded: EncodedContent) throws -> any Sendable {
        guard let text = String(data: encoded.content, encoding: .utf8) else {
            throw ConformanceFailure("custom content was not UTF-8")
        }
        return text
    }
}

struct SlashCodec: SDKContentCodec {
    let type = ContentTypeId(authorityId: "example.org", typeId: "a/b", versionMajor: 1, versionMinor: 0)

    func encode(_ value: any Sendable) throws -> EncodedContent {
        guard let text = value as? String else { throw ConformanceFailure("custom value was not text") }
        return EncodedContent(type: type, content: Data(text.utf8))
    }

    func decode(_: EncodedContent) throws -> any Sendable {
        "wrong codec"
    }
}

struct FailingCodec: SDKContentCodec {
    let type = SampleCodec().type
    func encode(_ value: any Sendable) throws -> EncodedContent {
        try SampleCodec().encode(value)
    }

    func decode(_: EncodedContent) throws -> any Sendable {
        throw ConformanceFailure("codec decode failed")
    }
}
