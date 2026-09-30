import CryptoKit
import Foundation
import XmtpSdk

func sha256Hex(_ bytes: Data) -> String {
    SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
}

/// The deployment directory name of an identifier, from the identifier alone.
func deploymentComponent(_ identifier: String) throws -> String {
    // A short printable identifier with no character the file name table
    // removes keeps its bytes, lowercased.
    let edges: Set<Character> = [".", " "]
    guard (1 ... 190).contains(identifier.count),
          identifier.unicodeScalars.allSatisfy({ (32 ... 126).contains($0.value) && !"<>:\"|?*/\\".unicodeScalars.contains($0) }),
          !edges.contains(identifier.first!), !edges.contains(identifier.last!)
    else { throw ConformanceFailure("the expected deployment directory assumes a file-safe identifier: \(identifier)") }
    return "\(identifier.lowercased())-\(sha256Hex(Data(identifier.utf8)))"
}

/// The loopback object store the backend presigns uploads for.
let objectStore = ProcessInfo.processInfo.environment["XMTP_S3_URL"] ?? "http://127.0.0.1:9067"

/// Store bytes on the object store and return their URL.
func servedObject(_ body: Data) throws -> String {
    let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try body.write(to: file)
    defer { try? FileManager.default.removeItem(at: file) }
    let url = "\(objectStore)/fixtures/\(UUID().uuidString)"
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
    process.arguments = ["--silent", "--fail", "--upload-file", file.path, url]
    try process.run()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else { throw ConformanceFailure("the object store refused a fixture") }
    return url
}

let attachmentKinds: [EventKind] = [
    .attachmentUploadStarted,
    .attachmentUploadCompleted,
    .attachmentUploadFailed,
    .attachmentDownloadStarted,
    .attachmentDownloadCompleted,
    .attachmentDownloadFailed,
    .attachmentDeleted,
]

func attachmentFilter(_ kinds: [EventKind] = attachmentKinds) -> EventFilter {
    EventFilter(
        kinds: kinds + [.conversationJoined], conversationIds: nil,
        contentTypes: nil, referencesOwnMessages: false
    )
}

/// One attachment event: its kind, the attachment, and a failure's cause.
struct AttachmentEvent: Equatable {
    var kind: EventKind
    var attachmentKey: String
    var url: String
    var contentDigest: String
    var cause: AttachmentFailureCause?

    init(_ kind: EventKind, _ attachment: AttachmentRef) {
        self.kind = kind
        attachmentKey = attachment.attachmentKey
        url = attachment.url
        contentDigest = attachment.contentDigest
    }

    init(_ kind: EventKind, _ attachment: AttachmentFailed) {
        self.kind = kind
        attachmentKey = attachment.attachmentKey
        url = attachment.url
        contentDigest = attachment.contentDigest
        cause = attachment.cause
    }

    /// This event with another kind and cause.
    func with(_ kind: EventKind, cause: AttachmentFailureCause? = nil) -> AttachmentEvent {
        var event = self
        event.kind = kind
        event.cause = cause
        return event
    }
}

func attachmentEvent(_ event: ClientEvent) -> AttachmentEvent? {
    switch event {
    case let .attachmentUploadStarted(attachment): AttachmentEvent(.attachmentUploadStarted, attachment)
    case let .attachmentUploadCompleted(attachment): AttachmentEvent(.attachmentUploadCompleted, attachment)
    case let .attachmentUploadFailed(attachment): AttachmentEvent(.attachmentUploadFailed, attachment)
    case let .attachmentDownloadStarted(attachment): AttachmentEvent(.attachmentDownloadStarted, attachment)
    case let .attachmentDownloadCompleted(attachment): AttachmentEvent(.attachmentDownloadCompleted, attachment)
    case let .attachmentDownloadFailed(attachment): AttachmentEvent(.attachmentDownloadFailed, attachment)
    case let .attachmentDeleted(attachment): AttachmentEvent(.attachmentDeleted, attachment)
    default: nil
    }
}

private final class Settle<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<T, Error>?

    init(_ continuation: CheckedContinuation<T, Error>) {
        self.continuation = continuation
    }

    func resume(_ result: Result<T, Error>) {
        lock.lock()
        let pending = continuation
        continuation = nil
        lock.unlock()
        pending?.resume(with: result)
    }
}

/// Run an action, failing when it has not finished after `seconds`.
func within<T: Sendable>(
    seconds: Int = 10, _ action: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withCheckedThrowingContinuation { continuation in
        let settle = Settle(continuation)
        let work = Task {
            do {
                let value = try await action()
                settle.resume(.success(value))
            } catch {
                settle.resume(.failure(error))
            }
        }
        Task {
            try? await Task.sleep(for: .seconds(seconds))
            work.cancel()
            settle.resume(.failure(ConformanceFailure("no result after \(seconds) seconds")))
        }
    }
}

private final class Held<T>: @unchecked Sendable {
    let value: T

    init(_ value: T) {
        self.value = value
    }
}

/// The next event of a reader, failing when none arrives within ten seconds.
private func within(next iterator: SDKEventStream.Iterator) async throws -> ClientEvent? {
    let held = Held(iterator)
    return try await within { try await held.value.next() }
}

/// Events of one reader, read one at a time with a bound on the wait.
final class EventQueue {
    private let iterator: SDKEventStream.Iterator

    init(_ client: SDKClient, _ filter: EventFilter = attachmentFilter()) async throws {
        iterator = try await client.events(filter).makeAsyncIterator()
    }

    func next() async throws -> ClientEvent {
        guard let event = try await within(next: iterator) else {
            throw ConformanceFailure("the event reader ended")
        }
        return event
    }

    func ended() async throws -> Bool {
        try await within(next: iterator) == nil
    }

    /// Read attachment events up to the group this creates, which marks the end.
    func drain(_ client: SDKClient) async throws -> [AttachmentEvent] {
        let marker = try await client.conversations().createGroup(members: [InboxId](), options: nil).id()
        var events: [AttachmentEvent] = []
        while true {
            let event = try await next()
            if case let .conversationJoined(conversationId, _, _, _) = event {
                if conversationId == marker {
                    return events
                }
                continue
            }
            guard let attachment = attachmentEvent(event) else {
                throw ConformanceFailure("unexpected event \(event)")
            }
            events.append(attachment)
        }
    }
}

func failure(
    _ cause: AttachmentFailureCause,
    credentialKind: CredentialFailureKind? = nil,
    retryable: Bool = false,
    missingScope: Bool = false,
    httpStatus: UInt16? = nil
) -> AttachmentFailure {
    AttachmentFailure(
        cause: cause, credentialKind: credentialKind, retryable: retryable,
        missingScope: missingScope, httpStatus: httpStatus
    )
}

/// The details and record of the attachment error an action throws.
func thrownAttachment(_ action: () async throws -> Void) async throws -> (ErrorDetails, AttachmentFailure) {
    do {
        try await action()
    } catch let XmtpError.Attachment(details, failure) {
        guard details.code == "Attachment" else { throw ConformanceFailure("attachment error code \(details.code)") }
        return (details, failure)
    } catch {
        throw ConformanceFailure("expected an attachment error, got \(error)")
    }
    throw ConformanceFailure("expected an attachment error, got success")
}

func thrownFailure(_ action: () async throws -> Void) async throws -> AttachmentFailure {
    try await thrownAttachment(action).1
}

func checkClientClosed(_ action: @escaping @Sendable () async throws -> Void) async throws {
    do {
        try await within(action)
    } catch XmtpError.ClientClosed {
        return
    } catch {
        throw ConformanceFailure("expected ClientClosed, got \(error)")
    }
    throw ConformanceFailure("expected ClientClosed, got success")
}
