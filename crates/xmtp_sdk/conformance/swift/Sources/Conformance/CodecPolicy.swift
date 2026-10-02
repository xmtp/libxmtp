import Foundation
import XmtpSdk

// The typed codec send policy (Ref Public surface, Host codecs; P9 and P10).

/// A standard codec's bytes equal Rust's, decoding them gives back an equal
/// value (the generated records compare every field), and re-encoding that
/// value gives the same bytes.
func matchesRust<C: ContentCodec>(
    _ codec: C, _ value: C.Value, _ expected: EncodedContent
) throws -> Bool where C.Value: Equatable {
    let encoded = try codec.encode(value)
    let decoded = try codec.decode(encoded)
    return try sameEncoded(encoded, expected) && decoded == value && sameEncoded(codec.encode(decoded), expected) &&
        codec.fallback(value) == expected.fallback &&
        codec.shouldPush(value) == catalogueContentTypeShouldPush(contentType: expected.type)
}

/// A codec with no value, such as the read receipt: only the bytes compare.
func matchesRust<C: ContentCodec>(
    _ codec: C, _ value: C.Value, _ expected: EncodedContent
) throws -> Bool where C.Value == Void {
    let encoded = try codec.encode(value)
    try codec.decode(encoded)
    return try sameEncoded(encoded, expected) && sameEncoded(codec.encode(()), expected) &&
        codec.fallback(value) == expected.fallback &&
        codec.shouldPush(value) == catalogueContentTypeShouldPush(contentType: expected.type)
}

private let noteType = ContentTypeId(authorityId: "example.org", typeId: "note", versionMajor: 1, versionMinor: 0)
private let emptyType = ContentTypeId(authorityId: "", typeId: "note", versionMajor: 1, versionMinor: 0)

private struct StepNotAllowed: Error {
    let step: String
}

/// A note codec. Each step can fail, return its own fallback, or change type.
private struct NoteCodec: ContentCodec {
    var failEncode = false
    var failFallback = false
    var failPush = false
    var ownFallback: String?
    var envelopeType = noteType
    var push = true
    var type = noteType

    func encode(_ value: String) throws -> EncodedContent {
        if failEncode {
            throw StepNotAllowed(step: "encode")
        }
        return EncodedContent(type: envelopeType, fallback: ownFallback, content: Data(value.utf8))
    }

    func decode(_ encoded: EncodedContent) throws -> String {
        String(decoding: encoded.content, as: UTF8.self)
    }

    func fallback(_ value: String) throws -> String? {
        if failFallback {
            throw StepNotAllowed(step: "fallback")
        }
        return "a note: \(value)"
    }

    func shouldPush(_: String) throws -> Bool {
        if failPush {
            throw StepNotAllowed(step: "shouldPush")
        }
        return push
    }
}

/// A codec of a catalogue type whose push hook must not run.
private struct CatalogueTextCodec: ContentCodec {
    var type: ContentTypeId {
        TextCodec().type
    }

    func encode(_ value: String) throws -> EncodedContent {
        try TextCodec().encode(value)
    }

    func decode(_ encoded: EncodedContent) throws -> String {
        try TextCodec().decode(encoded)
    }

    func shouldPush(_: String) throws -> Bool {
        throw StepNotAllowed(step: "shouldPush for a catalogue type")
    }
}

/// A Group with no Rust object that records the options each send receives.
private final class RecordingGroup: Group, @unchecked Sendable {
    private(set) var sent: [SendOptions?] = []

    init() {
        super.init(noHandle: Group.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func send(encoded _: EncodedContent, options: SendOptions? = nil) async throws -> MessageId {
        sent.append(options)
        return "recorded"
    }

    override func prepareMessage(encoded: EncodedContent, options: SendOptions? = nil) async throws -> MessageId {
        try await send(encoded: encoded, options: options)
    }
}

/// The push hook result reaches the send options, unless an option or the
/// catalogue decides.
private func checkPushOptions() async throws {
    let group = RecordingGroup()
    _ = try await group.send(NoteCodec(push: false), value: "quiet")
    _ = try await group.prepareMessage(NoteCodec(push: true), value: "loud")
    _ = try await group.send(
        NoteCodec(failPush: true), value: "explicit", options: SendOptions(shouldPush: false, optimistic: true)
    )
    _ = try await group.send(CatalogueTextCodec(), value: "catalogue")
    let pushes = group.sent.map { $0?.shouldPush }
    guard pushes == [false, true, false, nil]
    else { throw ConformanceFailure("the send options have the wrong push values: \(pushes)") }
    guard group.sent[2]?.optimistic == true
    else { throw ConformanceFailure("an explicit option lost its other fields") }
    try await checkTypeReadsAndCancellation()
}

/// A codec that counts reads of its type and can cancel its caller's task
/// from inside `encode`.
private final class ProbeCodec: ContentCodec, @unchecked Sendable {
    private let lock = NSLock()
    private var reads = 0
    let cancelInEncode: Bool
    let cancelInFallback: Bool
    let ownCancellation: Bool
    private(set) var encoded = false

    init(cancelInEncode: Bool = false, cancelInFallback: Bool = false, ownCancellation: Bool = false) {
        self.cancelInEncode = cancelInEncode
        self.cancelInFallback = cancelInFallback
        self.ownCancellation = ownCancellation
    }

    func fallback(_: String) throws -> String? {
        if ownCancellation {
            // The codec throws its own CancellationError in a live task.
            throw CancellationError()
        }
        if cancelInFallback {
            // The hook sees its own task cancelled and stops with CancellationError.
            withUnsafeCurrentTask { $0?.cancel() }
            try Task.checkCancellation()
        }
        return nil
    }

    var typeReads: Int {
        lock.withLock { reads }
    }

    var type: ContentTypeId {
        lock.withLock { reads += 1 }
        return noteType
    }

    func encode(_ value: String) throws -> EncodedContent {
        encoded = true
        if cancelInEncode {
            withUnsafeCurrentTask { $0?.cancel() }
        }
        return EncodedContent(type: noteType, content: Data(value.utf8))
    }

    func decode(_ encoded: EncodedContent) throws -> String {
        String(decoding: encoded.content, as: UTF8.self)
    }
}

/// The codec's type is read once per send, for the envelope check and the
/// push choice. A task cancelled during a codec step stops before the send.
private func checkTypeReadsAndCancellation() async throws {
    let group = RecordingGroup()
    let counted = ProbeCodec()
    _ = try await group.send(counted, value: "counted")
    guard counted.typeReads == 1
    else { throw ConformanceFailure("a typed send read the codec type \(counted.typeReads) times") }

    let cancelling = ProbeCodec(cancelInEncode: true)
    let sends = group.sent.count
    let outcome = await Task { try await group.send(cancelling, value: "cancelled") }.result
    guard cancelling.encoded, case let .failure(error) = outcome, error is CancellationError
    else { throw ConformanceFailure("a cancelled typed send did not stop: \(outcome)") }
    guard group.sent.count == sends
    else { throw ConformanceFailure("a cancelled typed send reached the send") }

    // A hook that checks its task's cancellation reports the caller's
    // cancellation unchanged, not as CodecEncodeFailed.
    let checking = ProbeCodec(cancelInFallback: true)
    let checked = await Task { try await group.send(checking, value: "checked") }.result
    guard case let .failure(error) = checked, error is CancellationError
    else { throw ConformanceFailure("a hook's CancellationError was not kept: \(checked)") }
    guard group.sent.count == sends
    else { throw ConformanceFailure("a hook cancelled in its task reached the send") }

    // A codec's own CancellationError in a task that is not cancelled is a
    // codec failure.
    let own = await Task { try await group.send(ProbeCodec(ownCancellation: true), value: "own") }.result
    guard case let .failure(error) = own, case XmtpError.CodecEncodeFailed = error
    else { throw ConformanceFailure("a codec's own CancellationError was not CodecEncodeFailed: \(own)") }
    guard group.sent.count == sends
    else { throw ConformanceFailure("a codec's own CancellationError reached the send") }
}

private func envelope(_ message: Message?) -> EncodedContent? {
    switch message?.content {
    case let .custom(encoded, _, _, _): encoded
    case let .unknown(encoded, _, _): encoded
    default: nil
    }
}

private func isCodecEncodeFailed(_ error: Error) -> Bool {
    guard case let XmtpError.CodecEncodeFailed(details) = error else { return false }
    return details.code == "CodecEncodeFailed" && details.category == .callback && !details.retryable
}

private func stored(_ group: Group, _ id: MessageId) async throws -> Message? {
    try await group.messages(options: nil).first { $0.id == id }
}

private func expectCodecEncodeFailed(_ what: String, _ attempt: () async throws -> MessageId) async throws {
    do {
        _ = try await attempt()
    } catch where isCodecEncodeFailed(error) {
        return
    }
    throw ConformanceFailure("\(what) did not fail with CodecEncodeFailed")
}

/// custom_codec_policy_and_isolation: a typed send, prepare, and reply apply
/// the codec's fallback and push hooks, an explicit push value wins, and a
/// client without the codec keeps the envelope. Returns the typed send, a
/// parent for the failure checks.
// verifies: CTYPE-017, CTYPE-021, SEND-021
func customCodecPolicyAndIsolation(group: Group, receiver: SDKClient) async throws -> Message {
    try await checkPushOptions()
    // A typed send fills the fallback.
    let sentId = try await group.send(NoteCodec(), value: "typed send")
    guard let parent = try await stored(group, sentId), envelope(parent)?.fallback == "a note: typed send"
    else { throw ConformanceFailure("a typed send did not fill the fallback") }

    // prepareMessage takes the codec form and stores an unpublished item.
    let preparedId = try await group.prepareMessage(NoteCodec(), value: "prepared")
    guard try await stored(group, preparedId)?.data.deliveryStatus == .unpublished
    else { throw ConformanceFailure("a typed prepareMessage did not store an unpublished item") }
    try await group.publishMessage(id: preparedId)

    // A typed reply fills the nested fallback and keeps the reply's push.
    let replyId = try await parent.reply(NoteCodec(failPush: true), value: "typed reply")
    guard case let .unknown(nested, _, _)? = try await stored(group, replyId)?.replyContent,
          nested?.fallback == "a note: typed reply"
    else { throw ConformanceFailure("a typed reply did not fill the nested fallback") }

    // A receiver without the codec keeps the envelope and its fallback.
    _ = try await receiver.conversations().syncAll(consentStates: nil)
    let received = try await receiver.conversations().getMessageById(id: sentId)
    guard case let .unknown(receivedEnvelope, _, _)? = received?.content,
          receivedEnvelope?.fallback == "a note: typed send"
    else { throw ConformanceFailure("a receiver without the codec lost the envelope") }
    return parent
}

/// codec_policy_failure_never_publishes: a skipped hook is not called, and a
/// failed encode, fallback, or shouldPush step, or an envelope of another
/// type, is CodecEncodeFailed with no publish attempt.
// verifies: CTYPE-003, CTYPE-007
func codecPolicyFailureNeverPublishes(group: Group, parent: Message) async throws {
    // An envelope's own fallback is kept, and its fallback hook is not called.
    let keptId = try await group.send(NoteCodec(failFallback: true, ownFallback: "own"), value: "kept")
    guard try await envelope(stored(group, keptId))?.fallback == "own"
    else { throw ConformanceFailure("an envelope fallback was replaced") }
    // An explicit shouldPush and a catalogue type skip the push hook.
    _ = try await group.send(NoteCodec(failPush: true), value: "explicit", options: SendOptions(shouldPush: false))
    _ = try await group.send(CatalogueTextCodec(), value: "catalogue text")

    // A failed step makes no publish attempt, on send, prepare, and reply.
    let before = try await group.messages(options: nil).count
    let failing: [NoteCodec] = [
        NoteCodec(failEncode: true),
        NoteCodec(failFallback: true),
        NoteCodec(failPush: true),
        NoteCodec(envelopeType: TextCodec().type),
        // The codec and its envelope agree; only the empty identifier fails.
        NoteCodec(envelopeType: emptyType, type: emptyType),
    ]
    for codec in failing {
        try await expectCodecEncodeFailed("send") { try await group.send(codec, value: "x") }
        try await expectCodecEncodeFailed("prepareMessage") { try await group.prepareMessage(codec, value: "x") }
    }
    try await expectCodecEncodeFailed("reply") { try await parent.reply(NoteCodec(failEncode: true), value: "x") }
    guard try await group.messages(options: nil).count == before else {
        throw ConformanceFailure("a failed codec step made a publish attempt")
    }
}
