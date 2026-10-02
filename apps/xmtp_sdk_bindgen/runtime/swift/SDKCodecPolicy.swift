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

/// A codec step runs synchronously inside the caller's task, so a
/// `CancellationError` from it (for example `Task.checkCancellation()` in a
/// hook) reports the caller's cancellation and passes through unchanged. Every
/// other error is `CodecEncodeFailed`. Kotlin differs on purpose: a Kotlin step
/// is not a suspend function and cannot see the caller's coroutine, so there a
/// codec's own `CancellationException` is `CodecEncodeFailed`.
private func step<R>(_ name: String, _ run: () throws -> R) throws -> R {
    do {
        return try run()
    } catch let cancelled as CancellationError where Task.isCancelled {
        // Only a cancelled task makes this the caller's cancellation. A
        // codec's own CancellationError() in a live task is a codec failure.
        throw cancelled
    } catch {
        throw codecEncodeFailed(name, error)
    }
}

private func sameType(_ left: ContentTypeId, _ right: ContentTypeId) -> Bool {
    left.authorityId == right.authorityId && left.typeId == right.typeId
        && left.versionMajor == right.versionMajor && left.versionMinor == right.versionMinor
}

/// The envelope of `value` for a send. An envelope that already has a fallback
/// keeps it, and `fallback` is not called; otherwise `fallback(value)` supplies
/// it. A failed step, or an envelope of another type than the codec's, is
/// `CodecEncodeFailed`.
///
/// `type` is the codec's type, read once by the caller and reused for the push
/// choice. The envelope is a value, so a hook cannot change the checked copy.
func encodeForSend<C: ContentCodec>(_ codec: C, value: C.Value, type: ContentTypeId? = nil) throws -> EncodedContent {
    let type = type ?? codec.type
    var encoded = try step("encode") { try codec.encode(value) }
    // implements: CTYPE-007
    // The envelope type is the codec's type, so a codec's push hook cannot
    // steer catalogue dispatch.
    guard sameType(encoded.type, type) else {
        throw codecEncodeFailed("encode", CodecStepFailure(description: "the envelope type differs from the codec type"))
    }
    // implements: CTYPE-003
    // An empty authority or type ID fails here, before the send, not in the
    // binding.
    guard !encoded.type.authorityId.isEmpty, !encoded.type.typeId.isEmpty else {
        throw codecEncodeFailed("encode", CodecStepFailure(description: "the envelope type has an empty authority or type ID"))
    }
    if encoded.fallback == nil && !usesRustStandardFallback(codec) {
        encoded.fallback = try step("fallback") { try codec.fallback(value) }
    }
    return encoded
}

/// The send options for `value`. An explicit `shouldPush`, including `false`,
/// wins. A catalogue type keeps its catalogue default. Otherwise the codec's
/// `shouldPush` decides. A failed hook is `CodecEncodeFailed`.
func optionsForSend<C: ContentCodec>(
    _ codec: C, value: C.Value, options: SendOptions?, type: ContentTypeId? = nil
) throws -> SendOptions? {
    if options?.shouldPush != nil || isCatalogueContentType(contentType: type ?? codec.type) {
        return options
    }
    var result = options ?? SendOptions()
    result.shouldPush = try step("shouldPush") { try codec.shouldPush(value) }
    return result
}

/// The envelope and options of `value` for a send or prepare. The codec's type
/// is read once. After the codec steps, a cancelled task stops here, before
/// the send starts.
func sendParts<C: ContentCodec>(
    _ codec: C, value: C.Value, options: SendOptions?
) throws -> (EncodedContent, SendOptions?) {
    let type = codec.type
    let encoded = try encodeForSend(codec, value: value, type: type)
    let sendOptions = try optionsForSend(codec, value: value, options: options, type: type)
    try Task.checkCancellation()
    return (encoded, sendOptions)
}

/// Decision 23: a typed codec form of send and prepareMessage next to the
/// envelope form.
public extension Group {
    func send<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let (encoded, sendOptions) = try sendParts(codec, value: value, options: options)
        return try await send(encoded: encoded, options: sendOptions)
    }

    func prepareMessage<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let (encoded, sendOptions) = try sendParts(codec, value: value, options: options)
        return try await prepareMessage(encoded: encoded, options: sendOptions)
    }
}

public extension Dm {
    func send<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let (encoded, sendOptions) = try sendParts(codec, value: value, options: options)
        return try await send(encoded: encoded, options: sendOptions)
    }

    func prepareMessage<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let (encoded, sendOptions) = try sendParts(codec, value: value, options: options)
        return try await prepareMessage(encoded: encoded, options: sendOptions)
    }
}

public extension Conversation {
    func send<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let (encoded, sendOptions) = try sendParts(codec, value: value, options: options)
        return try await send(encoded: encoded, options: sendOptions)
    }

    func prepareMessage<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let (encoded, sendOptions) = try sendParts(codec, value: value, options: options)
        return try await prepareMessage(encoded: encoded, options: sendOptions)
    }
}

// Standard encoders already apply their canonical fallback, including nil.
private func usesRustStandardFallback<C: ContentCodec>(_ codec: C) -> Bool {
    codec is TextCodec ||
        codec is MarkdownCodec ||
        codec is ReadReceiptCodec ||
        codec is ReactionV2Codec ||
        codec is AttachmentCodec ||
        codec is RemoteAttachmentCodec ||
        codec is MultiRemoteAttachmentCodec ||
        codec is TransactionReferenceCodec ||
        codec is WalletSendCallsCodec ||
        codec is ActionsCodec ||
        codec is IntentCodec ||
        codec is ReplyCodec ||
        codec is GroupUpdatedCodec ||
        codec is DeleteMessageCodec ||
        codec is LeaveRequestCodec
}
