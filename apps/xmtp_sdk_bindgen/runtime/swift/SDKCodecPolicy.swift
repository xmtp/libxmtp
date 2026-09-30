import Foundation

// The host send policy for typed codecs (Ref Public surface, Host codecs;
// Decisions 23 and 24). Every codec step runs before the send starts, so a
// failed step makes no publish attempt.

private func codecEncodeFailed(_ step: String, _ cause: Error) -> XmtpError {
    .CodecEncodeFailed(ErrorDetails(
        code: "CodecEncodeFailed", category: .callback, retryable: false,
        message: "content codec \(step) failed: \(cause)"
    ))
}

private struct CodecStepFailure: Error, CustomStringConvertible {
    let description: String
}

private func step<R>(_ name: String, _ run: () throws -> R) throws -> R {
    do { return try run() } catch { throw codecEncodeFailed(name, error) }
}

private func sameType(_ left: ContentTypeId, _ right: ContentTypeId) -> Bool {
    left.authorityId == right.authorityId && left.typeId == right.typeId
        && left.versionMajor == right.versionMajor && left.versionMinor == right.versionMinor
}

/// The envelope of `value` for a send. An envelope that already has a fallback
/// keeps it, and `fallback` is not called; otherwise `fallback(value)` supplies
/// it. A failed step, or an envelope of another type than the codec's, is
/// `CodecEncodeFailed`.
func encodeForSend<C: ContentCodec>(_ codec: C, value: C.Value) throws -> EncodedContent {
    var encoded = try step("encode") { try codec.encode(value) }
    // implements: CTYPE-007
    // The envelope type is the codec's type, so a codec's push hook cannot
    // steer catalogue dispatch.
    guard sameType(encoded.type, codec.type) else {
        throw codecEncodeFailed("encode", CodecStepFailure(description: "the envelope type differs from the codec type"))
    }
    if encoded.fallback == nil {
        encoded.fallback = try step("fallback") { try codec.fallback(value) }
    }
    return encoded
}

/// The send options for `value`. An explicit `shouldPush`, including `false`,
/// wins. A catalogue type keeps its catalogue default. Otherwise the codec's
/// `shouldPush` decides. A failed hook is `CodecEncodeFailed`.
func optionsForSend<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions?) throws -> SendOptions? {
    if options?.shouldPush != nil || isCatalogueContentType(contentType: codec.type) {
        return options
    }
    var result = options ?? SendOptions()
    result.shouldPush = try step("shouldPush") { try codec.shouldPush(value) }
    return result
}

/// Decision 23: a typed codec form of send and prepareMessage next to the
/// envelope form.
public extension Group {
    func send<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let encoded = try encodeForSend(codec, value: value)
        return try await send(encoded: encoded, options: optionsForSend(codec, value: value, options: options))
    }

    func prepareMessage<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let encoded = try encodeForSend(codec, value: value)
        return try await prepareMessage(encoded: encoded, options: optionsForSend(codec, value: value, options: options))
    }
}

public extension Dm {
    func send<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let encoded = try encodeForSend(codec, value: value)
        return try await send(encoded: encoded, options: optionsForSend(codec, value: value, options: options))
    }

    func prepareMessage<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let encoded = try encodeForSend(codec, value: value)
        return try await prepareMessage(encoded: encoded, options: optionsForSend(codec, value: value, options: options))
    }
}

public extension Conversation {
    func send<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let encoded = try encodeForSend(codec, value: value)
        return try await send(encoded: encoded, options: optionsForSend(codec, value: value, options: options))
    }

    func prepareMessage<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let encoded = try encodeForSend(codec, value: value)
        return try await prepareMessage(encoded: encoded, options: optionsForSend(codec, value: value, options: options))
    }
}
