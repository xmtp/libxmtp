import Foundation
@testable import XmtpSdk

struct ConformanceFailure: LocalizedError {
    let errorDescription: String?

    init(_ check: String) {
        errorDescription = check
    }
}

private func sameEncoded(_ lhs: EncodedContent, _ rhs: EncodedContent) -> Bool {
    lhs.type.authorityID == rhs.type.authorityID &&
        lhs.type.typeID == rhs.type.typeID &&
        lhs.type.versionMajor == rhs.type.versionMajor &&
        lhs.type.versionMinor == rhs.type.versionMinor &&
        lhs.parameters == rhs.parameters &&
        lhs.fallback == rhs.fallback &&
        lhs.content == rhs.content
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

struct SampleCodec: SDKContentCodec {
    let type = ContentTypeID(authorityID: "example.org", typeID: "sample", versionMajor: 1, versionMinor: 0)
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

struct FailingCodec: SDKContentCodec {
    let type = SampleCodec().type
    func encode(_ value: any Sendable) throws -> EncodedContent {
        try SampleCodec().encode(value)
    }

    func decode(_: EncodedContent) throws -> any Sendable {
        throw ConformanceFailure("codec decode failed")
    }
}

@main
struct Conformance {
    static func main() async throws {
        precondition(sdkVersion().hasPrefix("1.12.0"))
        let messageID = try MessageID.fromString(String(repeating: "a", count: 64))
        precondition(messageID.description.count == 64)
        do {
            _ = try MessageID.fromString("bad")
            throw ConformanceFailure("malformed ID was accepted")
        } catch XmtpError.InvalidArgument {}
        print("Swift scenario 1: load, checksums, version passed")

        let codecSamples = sdkConformanceStandardSamples()
        guard codecSamples.count == 15 else { throw ConformanceFailure("missing standard codec samples") }
        for sample in codecSamples {
            let codec: any SDKContentCodec
            let value: any Sendable
            switch sample.value {
            case let .text(item): codec = TextCodec(); value = item
            case let .markdown(item): codec = MarkdownCodec(); value = item
            case .readReceipt: codec = ReadReceiptCodec(); value = ()
            case .reaction: codec = ReactionV2Codec(); value = sample.value
            case let .attachment(item): codec = AttachmentCodec(); value = item
            case let .remoteAttachment(item): codec = RemoteAttachmentCodec(); value = item
            case let .multiRemoteAttachment(item): codec = MultiRemoteAttachmentCodec(); value = item
            case let .transactionReference(item): codec = TransactionReferenceCodec(); value = item
            case let .walletSendCalls(item): codec = WalletSendCallsCodec(); value = item
            case let .actions(item): codec = ActionsCodec(); value = item
            case let .intent(item): codec = IntentCodec(); value = item
            case .reply: codec = ReplyCodec(); value = sample.value
            case let .groupUpdated(item): codec = GroupUpdatedCodec(); value = item
            case .deleteMessage: codec = DeleteMessageCodec(); value = sample.value
            case let .leaveRequest(item): codec = LeaveRequestCodec(); value = item
            }
            let encoded = try codec.encode(value)
            guard sameEncoded(encoded, sample.expected),
                  try sameEncoded(codec.encode(codec.decode(encoded)), sample.expected)
            else { throw ConformanceFailure("standard codec bytes differ from Rust") }
        }
        print("Swift P69: all 15 standard codecs match Rust bytes")

        let signer = TestSigner()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("xmtp-sdk-conformance-\(UUID().uuidString)")
        let backendOptions = BackendOptions(url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"]!)
        precondition(ClientOptions(storage: StorageOptions(location: .inMemory)).backend == nil)
        let options = ClientOptions(
            backend: .options(options: backendOptions),
            storage: StorageOptions(location: .directory(directory.path)),
            deviceSync: false
        )
        let host = try await SDKClient.create(signer: signer, options: options)
        let client = host.raw
        let inboxID = client.inboxID()
        guard let storagePath = try await host.storage().path(),
              FileManager.default.fileExists(atPath: storagePath)
        else { throw ConformanceFailure("storage path does not name the database file") }
        let group = try await client.conversations().createGroup(members: [], options: nil)
        var typedSends = 0
        for sample in codecSamples {
            let id: MessageID
            switch sample.value {
            case let .text(text): id = try await group.sendText(text: text, options: nil)
            case let .markdown(markdown): id = try await group.sendMarkdown(markdown: markdown, options: nil)
            case let .reaction(reference, inboxID, reaction): id = try await group.sendReaction(reference: reference, referenceInboxID: inboxID, reaction: reaction, options: nil)
            case let .reply(reference, inboxID, content): id = try await group.sendReply(reference: reference, referenceInboxID: inboxID, content: content, options: nil)
            case .readReceipt: id = try await group.sendReadReceipt(options: nil)
            case let .attachment(attachment): id = try await group.sendAttachment(attachment: attachment, options: nil)
            case let .remoteAttachment(attachment): id = try await group.sendRemoteAttachment(attachment: attachment, options: nil)
            case let .multiRemoteAttachment(attachment): id = try await group.sendMultiRemoteAttachment(attachment: attachment, options: nil)
            case let .transactionReference(reference): id = try await group.sendTransactionReference(reference: reference, options: nil)
            case let .walletSendCalls(calls): id = try await group.sendWalletSendCalls(calls: calls, options: nil)
            case let .actions(actions): id = try await group.sendActions(actions: actions, options: nil)
            case let .intent(intent): id = try await group.sendIntent(intent: intent, options: nil)
            default: continue
            }
            guard let wire = try await client.conversations().getMessageByID(id: id),
                  sameEncoded(wire.encoded, sample.expected)
            else { throw ConformanceFailure("typed send bytes differ from codec") }
            typedSends += 1
        }
        guard typedSends == 12 else { throw ConformanceFailure("missing typed send cases") }
        print("Swift P69: typed send bytes match all 12 public codecs")
        let sentID = try await group.sendText(text: "conformance message", options: nil)
        let sent = try await group.messages(options: nil).first { $0.id == sentID }
        precondition(sent != nil)
        let owningClient = try sent?.client()
        precondition(owningClient === host)
        try await host.end()
        do {
            _ = try sent?.client()
            preconditionFailure("ended client remained in the registry")
        } catch XmtpError.ClientClosed {}
        do {
            _ = try await sent?.refresh()
            throw ConformanceFailure("message action after end did not fail")
        } catch XmtpError.ClientClosed {}
        let reopenedHost = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxID: inboxID
        )
        let reopened = reopenedHost.raw
        precondition(reopened.inboxID() == inboxID)
        let appName = "xmtp-sdk-conformance-\(UUID().uuidString)"
        let appFolder = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent(appName)
        defer { try? FileManager.default.removeItem(at: appFolder) }
        let defaultHost = try await SDKClient.build(
            identity: await signer.identity(),
            options: ClientOptions(
                backend: options.backend,
                storage: StorageOptions(location: .default),
                deviceSync: false
            ), inboxID: inboxID, appName: appName
        )
        let defaultFolder = appFolder.appendingPathComponent("xmtp")
        let defaultFiles = try FileManager.default.contentsOfDirectory(atPath: defaultFolder.path)
        guard defaultFiles.contains(where: { $0.hasSuffix(".db3") }) else {
            throw ConformanceFailure("Default storage has no database file")
        }
        try await defaultHost.end()
        try FileManager.default.removeItem(at: appFolder)
        var orphan: Message!
        weak var weakHost: SDKClient?
        do {
            let shortLived = try await SDKClient.build(
                identity: await signer.identity(), options: options, inboxID: inboxID
            )
            weakHost = shortLived
            let shortGroup = try await shortLived.raw.conversations().createGroup(members: [], options: nil)
            let orphanID = try await shortGroup.sendText(text: "weak owner", options: nil)
            orphan = try await shortGroup.messages(options: nil).first { $0.id == orphanID }
        }
        precondition(weakHost == nil, "the registry kept the host client alive")
        do {
            _ = try orphan.client()
            preconditionFailure("released client remained in the registry")
        } catch XmtpError.ClientClosed {}
        do {
            _ = try await orphan.refresh()
            throw ConformanceFailure("message action after release did not fail")
        } catch XmtpError.ClientClosed {}
        print("Swift client_closed_after_end_and_release passed")
        print("Swift scenario 2: create, reopen, end passed")

        let reopenedGroup = try await reopened.conversations().createGroup(members: [], options: nil)
        let reader = try await reopenedGroup.messageReader()
        let liveID = try await reopenedGroup.sendText(text: "durable stream", options: nil)
        let first = try await reader.next()
        precondition(first?.id == liveID)
        try await reader.end()
        let replay = try await reopenedGroup.messageReader()
        let repeated = try await replay.next()
        precondition(repeated?.id == liveID)
        let pending = Task { try await replay.next() }
        try await Task.sleep(for: .milliseconds(50))
        pending.cancel()
        try await replay.end()
        _ = try? await pending.value
        let stream = try await reopenedHost.messages(in: reopenedGroup)
        let adapterID = try await reopenedGroup.sendText(text: "adapter stream", options: nil)
        let iterator = stream.makeAsyncIterator()
        let fromAdapter = try await iterator.next()
        precondition(fromAdapter?.id == adapterID)
        let idle = Task { try await iterator.next() }
        try await Task.sleep(for: .milliseconds(50))
        idle.cancel()
        _ = try? await idle.value
        let protocolGroup = try await reopened.conversations().createGroup(members: [], options: nil)
        let firstID = try await protocolGroup.sendText(text: "ack on request", options: nil)
        do {
            let protocolStream = try await reopenedHost.messages(in: protocolGroup)
            for try await value in protocolStream {
                precondition(value.id == firstID)
                break
            }
        }
        try await Task.sleep(for: .milliseconds(100))
        let reread = try await protocolGroup.messageReader()
        let stopReplay = Task {
            try await Task.sleep(for: .seconds(3))
            try? await reread.end()
        }
        let replayed = try await reread.next()
        stopReplay.cancel()
        precondition(replayed?.id == firstID, "adapter prefetched and acknowledged a value")
        try await reread.end()
        let secondID = try await protocolGroup.sendText(text: "second request", options: nil)
        do {
            let protocolStream = try await reopenedHost.messages(in: protocolGroup)
            var protocolIterator: SDKMessageStream.Iterator? = protocolStream.makeAsyncIterator()
            let firstAgain = try await protocolIterator?.next()
            precondition(firstAgain?.id == firstID)
            let second = try await protocolIterator?.next()
            precondition(second?.id == secondID)
            protocolIterator = nil
        }
        try await Task.sleep(for: .milliseconds(100))
        let afterAck = try await protocolGroup.messageReader()
        let remaining = try await afterAck.next()
        precondition(remaining?.id == secondID, "adapter did not acknowledge on next request")
        try await afterAck.end()
        let (opened, openedSignal) = AsyncStream<MessageReader>.makeStream()
        let (release, releaseSignal) = AsyncStream<Void>.makeStream()
        SDKClient.readerOpenedForTest = { reader in
            openedSignal.yield(reader)
            var iterator = release.makeAsyncIterator()
            _ = await iterator.next()
        }
        let cancelledOpening = Task { try await reopenedHost.messages(in: protocolGroup) }
        var openedIterator = opened.makeAsyncIterator()
        guard let lateReader = await openedIterator.next() else {
            throw ConformanceFailure("reader did not open before cancellation")
        }
        cancelledOpening.cancel()
        releaseSignal.yield(())
        do {
            _ = try await cancelledOpening.value
            throw ConformanceFailure("cancelled reader creation returned a stream")
        } catch is CancellationError {}
        SDKClient.readerOpenedForTest = nil
        guard try await lateReader.next() == nil else {
            throw ConformanceFailure("late reader was not closed")
        }
        let reopenedReader = try await protocolGroup.messageReader()
        try await reopenedReader.end()
        print("Swift scenario 7: durable stream and idle cancellation passed")

        let largeExpiry: Int64 = 9_007_199_254_740_993
        let credentialOptions = ClientOptions(
            backend: .options(options: BackendOptions(
                url: backendOptions.url,
                credential: Credential(name: nil, value: "Bearer initial", expiresAtSeconds: largeExpiry)
            )),
            storage: StorageOptions(location: .inMemory),
            deviceSync: false
        )
        let credentialHost = try await SDKClient.build(
            identity: await signer.identity(), options: credentialOptions, inboxID: inboxID
        )
        guard case let .some(.options(options: savedBackend)) = credentialHost.raw.options().backend,
              savedBackend.credential?.expiresAtSeconds == largeExpiry
        else {
            throw ConformanceFailure("credential expiry lost 64-bit precision")
        }
        try await credentialHost.raw.setCredential(credential: Credential(
            name: nil, value: "Bearer renewed", expiresAtSeconds: largeExpiry
        ))
        try await credentialHost.end()
        print("Swift scenario 3: credential update and 64-bit value passed")

        let snapshot = reopened.serverConfiguration()
        let fetched = try await fetchServerConfiguration(backend: .options(options: backendOptions))
        let staticBackend = try await Backend.connect(options: backendOptions)
        let staticIdentity = try await signer.identity()
        guard try await SDKClient.inboxID(for: staticIdentity, backend: .connected(backend: staticBackend)) == inboxID else {
            throw ConformanceFailure("backend-only inbox lookup returned a different ID")
        }
        guard try await SDKClient.canMessage([staticIdentity], backend: .connected(backend: staticBackend)).first?.canMessage == true else {
            throw ConformanceFailure("backend-only canMessage did not find this inbox")
        }
        guard try await SDKClient.canMessage([staticIdentity], backend: .options(options: backendOptions)).first?.canMessage == true else {
            throw ConformanceFailure("backend options canMessage did not find this inbox")
        }
        let connectedHost = try await SDKClient.build(
            identity: staticIdentity,
            options: ClientOptions(backend: .connected(backend: staticBackend), storage: StorageOptions(location: .inMemory), deviceSync: false),
            inboxID: inboxID
        )
        try await connectedHost.end()
        guard snapshot.identifier == fetched.identifier else {
            throw ConformanceFailure("configuration fetch returned a different deployment")
        }
        let refreshed = try await reopened.refreshServerConfiguration()
        guard refreshed.identifier == snapshot.identifier else {
            throw ConformanceFailure("configuration refresh returned a different deployment")
        }
        do {
            _ = try await fetchServerConfiguration(backend: .options(options: BackendOptions(url: "http://127.0.0.1:1")))
            throw ConformanceFailure("unavailable configuration request succeeded")
        } catch XmtpError.ConfigurationUnavailable {}
        print("Swift scenario 10: configuration and typed error passed")

        let local = await generateLocalSigner()
        let unsignedOptions = ClientOptions(
            backend: options.backend,
            storage: StorageOptions(location: .inMemory),
            deviceSync: false,
            registration: RegistrationOptions(auto: false)
        )
        let unsignedHost = try await SDKClient.create(signer: local, options: unsignedOptions)
        let unsigned = unsignedHost.raw
        guard try await !unsigned.isRegistered() else {
            throw ConformanceFailure("auto registration was not disabled")
        }
        guard let request = try await unsigned.unsafeCreateInboxSignatureRequest() else {
            throw ConformanceFailure("new inbox has no signature request")
        }
        guard !(await request.signatureText()).isEmpty else {
            throw ConformanceFailure("signature request has no text")
        }
        try await request.sign(signer: local)
        try await unsigned.unsafeApplySignatureRequest(request: request)
        guard try await unsigned.isRegistered() else {
            throw ConformanceFailure("signed inbox was not registered")
        }
        try await unsignedHost.end()
        print("Swift scenario 11: local signer and signature request passed")

        guard try reopened.notificationState() == .disabled else {
            throw ConformanceFailure("new client notification state was not disabled")
        }
        do {
            _ = try await reopened.enableNotifications(config: NotificationConfig(
                channel: .http(url: "https://example.com", signingKey: Data([1]))
            ))
            throw ConformanceFailure("invalid notification key was accepted")
        } catch XmtpError.InvalidArgument {}
        print("Swift scenario 12: notification state and typed error passed")

        try await initLogging(options: LoggingOptions(level: .error))
        let orderedSink = OrderedLogSink()
        try setLogSink(sink: orderedSink)
        try await sdkConformanceEmit(count: 32)
        guard orderedSink.sequence() == (0 ..< 32).map(String.init) else {
            throw ConformanceFailure("inline log sink changed record order")
        }
        try clearLogSink()
        print("Swift logging: inline records stayed in order")

        let family = try await reopened.conversations().createGroup(
            members: [], options: CreateGroupOptions(name: "family group")
        )
        guard try await family.state().name == "family group",
              family.creatorInboxID() == inboxID,
              try await reopened.conversations().listGroups(options: nil).contains(where: { $0.id() == family.id() })
        else { throw ConformanceFailure("group options, immutable fields, or list failed") }
        print("Swift scenario 4: group options, state, and list passed")

        let parentID = try await family.sendText(text: "parent", options: nil)
        let reactionID = try await reopened.conversations().reactToMessage(
            id: parentID, reaction: Reaction(content: "👍", action: .added, schema: .unicode), options: nil
        )
        let replyID = try await reopened.conversations().replyToMessage(
            id: parentID, content: encodeText(text: "reply"), options: nil
        )
        guard case .text = try await reopened.decodeContent(encoded: encodeText(text: "decoded"))
        else { throw ConformanceFailure("standard content did not decode") }
        let familyMessages = try await family.messages(options: nil)
        guard let parent = familyMessages.first(where: { $0.id == parentID }),
              let reply = familyMessages.first(where: { $0.id == replyID }),
              parent.replyCount == 1, parent.reactions.first?.id == reactionID,
              reply.inReplyTo?.id == parentID
        else { throw ConformanceFailure("message reaction or reply edge was not materialized") }
        let sameParent = try await reopened.conversations().getMessageByID(id: parentID)
        guard let sameParent, parent == sameParent else {
            throw ConformanceFailure("message_copies_compare_equal failed")
        }
        var changedStatus = parent.data
        changedStatus.deliveryStatus = parent.deliveryStatus == .failed ? .published : .failed
        guard parent != Message(data: changedStatus) else {
            throw ConformanceFailure("status_change_compares_unequal failed")
        }
        let forwarded: Conversation = .group(group: family)
        let forwardedLast = try await forwarded.lastMessage()
        let directLast = try await family.lastMessage()
        guard forwarded.id() == family.id(),
              forwardedLast?.id == directLast?.id
        else { throw ConformanceFailure("Conversation forwarding failed") }
        print("Swift message_copies_compare_equal and status_change_compares_unequal passed")
        print("Swift scenario 5: message records, reaction, and reply passed")

        let codec = SampleCodec()
        let withCodec = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxID: inboxID, codecs: [codec]
        )
        let withoutCodec = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxID: inboxID
        )
        let customID = try await family.send(encoded: codec.encode("codec value"), options: nil)
        guard let decoded = try await withCodec.raw.conversations().getMessageByID(id: customID),
              let undecoded = try await withoutCodec.raw.conversations().getMessageByID(id: customID)
        else { throw ConformanceFailure("custom message was not found") }
        guard case let .custom(_, value, nil) = decoded.content, value as? String == "codec value",
              case .unknown = undecoded.content
        else { throw ConformanceFailure("custom codec leaked between clients") }
        let customReplyID = try await withCodec.raw.conversations().replyToMessage(
            id: customID, content: codec.encode("reply codec value"), options: nil
        )
        guard let customReply = try await withCodec.raw.conversations().getMessageByID(id: customReplyID),
              case let .some(.custom(_, value, nil)) = customReply.replyContent,
              value as? String == "reply codec value"
        else { throw ConformanceFailure("reply body custom codec did not run") }
        let failingHost = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxID: inboxID, codecs: [FailingCodec()]
        )
        guard let failed = try await failingHost.raw.conversations().getMessageByID(id: customID),
              case let .custom(_, nil, error) = failed.content, error != nil
        else { throw ConformanceFailure("throwing custom codec was not recorded") }
        try await failingHost.end()
        print("Swift codec_scoped_to_client passed")
        try await withCodec.end()
        try await withoutCodec.end()
        print("Swift scenario 6: custom codec stayed with its client")

        let archive = try await reopened.archives().exportToBytes(keyBytes: Data(repeating: 7, count: 32), options: nil)
        guard !archive.isEmpty,
              try await reopened.archives().metadataFromBytes(data: archive, keyBytes: Data(repeating: 7, count: 32)).backupVersion == 0
        else { throw ConformanceFailure("archive byte export or metadata failed") }
        let archiveFolder = FileManager.default.temporaryDirectory.appendingPathComponent("xmtp-sdk-archive-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: archiveFolder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: archiveFolder) }
        let archivePath = archiveFolder.appendingPathComponent("snapshot.xmtp").path
        _ = try await reopened.archives().exportToFile(path: archivePath, keyBytes: Data(repeating: 7, count: 32), options: nil)
        guard try await reopened.archives().metadataFromFile(path: archivePath, keyBytes: Data(repeating: 7, count: 32)).backupVersion == 0
        else { throw ConformanceFailure("archive file export or metadata failed") }
        print("Swift scenario 9: archive bytes and file passed")

        try await reopenedHost.end()
    }
}
