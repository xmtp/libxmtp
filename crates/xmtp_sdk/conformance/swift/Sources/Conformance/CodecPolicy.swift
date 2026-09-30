import Foundation
import XmtpSdk

// The typed codec send policy (Ref Public surface, Host codecs; P9 and P10).

/// A standard codec's bytes equal Rust's, and a decode round trip keeps them.
func matchesRust<C: ContentCodec>(_ codec: C, _ value: C.Value, _ expected: EncodedContent) throws -> Bool {
    let encoded = try codec.encode(value)
    return try sameEncoded(encoded, expected) && sameEncoded(codec.encode(codec.decode(encoded)), expected)
}

private let noteType = ContentTypeId(authorityId: "example.org", typeId: "note", versionMajor: 1, versionMinor: 0)

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
    let type = noteType

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

private func envelope(_ message: Message?) -> EncodedContent? {
    switch message?.content {
    case let .custom(encoded, _, _): encoded
    case let .unknown(encoded): encoded
    default: nil
    }
}

private func isCodecEncodeFailed(_ error: Error) -> Bool {
    guard case let XmtpError.CodecEncodeFailed(details) = error else { return false }
    return details.code == "CodecEncodeFailed" && details.category == .callback && !details.retryable
}

// verifies: CTYPE-017, CTYPE-021
func checkCodecPolicy(group: Group, receiver: SDKClient) async throws {
    @Sendable func stored(_ id: MessageId) async throws -> Message? {
        try await group.messages(options: nil).first { $0.id == id }
    }
    // A typed send fills the fallback; an envelope's own fallback is kept.
    let sentId = try await group.send(NoteCodec(), value: "typed send")
    let sent = try await stored(sentId)
    guard envelope(sent)?.fallback == "a note: typed send"
    else { throw ConformanceFailure("a typed send did not fill the fallback") }
    let keptId = try await group.send(NoteCodec(failFallback: true, ownFallback: "own"), value: "kept")
    let kept = try await stored(keptId)
    guard envelope(kept)?.fallback == "own"
    else { throw ConformanceFailure("an envelope fallback was replaced") }

    // An explicit shouldPush and a catalogue type skip the push hook.
    _ = try await group.send(NoteCodec(failPush: true), value: "explicit", options: SendOptions(shouldPush: false))
    _ = try await group.send(CatalogueTextCodec(), value: "catalogue text")

    // prepareMessage takes the codec form and stores an unpublished item.
    let preparedId = try await group.prepareMessage(NoteCodec(), value: "prepared")
    let prepared = try await stored(preparedId)
    guard prepared?.data.deliveryStatus == .unpublished
    else { throw ConformanceFailure("a typed prepareMessage did not store an unpublished item") }
    try await group.publishMessage(id: preparedId)

    // A typed reply fills the nested fallback.
    guard let parent = sent else {
        throw ConformanceFailure("the typed send was not stored")
    }
    _ = try await parent.reply(NoteCodec(failPush: true), value: "typed reply")

    // A receiver without the codec keeps the envelope and its fallback.
    _ = try await receiver.conversations().syncAll(consentStates: nil)
    let received = try await receiver.conversations().getMessageById(id: sentId)
    guard case let .unknown(receivedEnvelope)? = received?.content,
          receivedEnvelope.fallback == "a note: typed send"
    else { throw ConformanceFailure("a receiver without the codec lost the envelope") }

    // A failed step makes no publish attempt, on send, prepare, and reply.
    let before = try await group.messages(options: nil).count
    let failing: [NoteCodec] = [
        NoteCodec(failEncode: true),
        NoteCodec(failFallback: true),
        NoteCodec(failPush: true),
        NoteCodec(envelopeType: TextCodec().type),
    ]
    for codec in failing {
        do {
            _ = try await group.send(codec, value: "x")
            throw ConformanceFailure("a failed codec step sent")
        } catch where isCodecEncodeFailed(error) {}
        do {
            _ = try await group.prepareMessage(codec, value: "x")
            throw ConformanceFailure("a failed codec step prepared a message")
        } catch where isCodecEncodeFailed(error) {}
    }
    do {
        _ = try await parent.reply(NoteCodec(failEncode: true), value: "x")
        throw ConformanceFailure("a failed codec step replied")
    } catch where isCodecEncodeFailed(error) {}
    let after = try await group.messages(options: nil).count
    guard after == before else {
        throw ConformanceFailure("a failed codec step made a publish attempt")
    }
}
