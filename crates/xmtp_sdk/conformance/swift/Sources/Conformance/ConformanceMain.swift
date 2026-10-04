import Foundation
@testable import XmtpSdk

@main
struct Conformance {
    static func main() async throws {
        setbuf(stdout, nil)
        if ProcessInfo.processInfo.environment["SDK_SWIFT_CALLER_CANCELLATION"] == "1" {
            try await checkSwiftCallerCancellation(backend: BackendOptions(url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"]!))
            return
        }
        if ProcessInfo.processInfo.environment["SDK_CALLBACK_LIFETIME"] == "1" {
            try await checkCallbackLifetime(backend: BackendOptions(url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"]!))
            return
        }
        if CommandLine.arguments.contains("--missing-bundle") {
            guard Bundle.main.bundleIdentifier == nil else {
                throw ConformanceFailure("bare executable unexpectedly has a bundle identifier")
            }
            do {
                _ = try await SDKClient.create(
                    signer: TestSigner(),
                    options: ClientOptions(storage: StorageOptions(location: .default))
                )
                throw ConformanceFailure("default storage accepted a missing bundle identifier")
            } catch XmtpError.StorageLocationRequired {}
            print("Swift missing bundle identifier rejected")
            return
        }
        precondition(sdkVersion().hasPrefix("1.12.0"))
        let messageId: MessageId = String(repeating: "a", count: 64)
        precondition(messageId.count == 64)
        print("Swift scenario 1: load, checksums, version passed")

        // Client-free standard codecs encode the client's bytes and round trip.
        // verifies: CTYPE-007, CTYPE-026
        let codecSamples = sdkConformanceStandardSamples()
        guard codecSamples.count == 15 else { throw ConformanceFailure("missing standard codec samples") }
        try checkCodecRecordValues()
        for sample in codecSamples {
            let expected = sample.expected
            let matches: Bool
            switch sample.value {
            case let .text(item): matches = try matchesRust(TextCodec(), item, expected)
            case let .markdown(item): matches = try matchesRust(MarkdownCodec(), item, expected)
            case .readReceipt: matches = try matchesRust(ReadReceiptCodec(), (), expected)
            case let .reaction(reference, inbox, reaction): matches = try matchesRust(ReactionV2Codec(), ReactionV2Content(reference: reference, referenceInboxId: inbox, reaction: reaction), expected)
            case let .attachment(item): matches = try matchesRust(AttachmentCodec(), item, expected)
            case let .remoteAttachment(item): matches = try matchesRust(RemoteAttachmentCodec(), item, expected)
            case let .multiRemoteAttachment(item): matches = try matchesRust(MultiRemoteAttachmentCodec(), item, expected)
            case let .transactionReference(item): matches = try matchesRust(TransactionReferenceCodec(), item, expected)
            case let .walletSendCalls(item): matches = try matchesRust(WalletSendCallsCodec(), item, expected)
            case let .actions(item): matches = try matchesRust(ActionsCodec(), item, expected)
            case let .intent(item): matches = try matchesRust(IntentCodec(), item, expected)
            case let .reply(reference, inbox, content): matches = try matchesRust(ReplyCodec(), ReplyContent(reference: reference, referenceInboxId: inbox, content: content), expected)
            case let .groupUpdated(item): matches = try matchesRust(GroupUpdatedCodec(), item, expected)
            case let .deleteMessage(messageId): matches = try matchesRust(DeleteMessageCodec(), DeleteMessageContent(messageId: messageId), expected)
            case let .leaveRequest(item): matches = try matchesRust(LeaveRequestCodec(), item, expected)
            }
            guard matches else { throw ConformanceFailure("standard codec bytes differ from Rust") }
        }
        print("Swift P69: all 15 standard codecs match Rust bytes")

        let signer = TestSigner()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("xmtp-sdk-conformance-\(UUID().uuidString)")
        let backendOptions = BackendOptions(url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"]!)
        precondition(ClientOptions(storage: StorageOptions(location: .inMemory)).backend == nil)
        let options = ClientOptions(
            backend: .options(options: backendOptions),
            storage: StorageOptions(location: .directory(directory: directory.path)),
            deviceSync: false
        )
        try await within(seconds: 30) { try await loggingConformance(options) }
        try await checkReaderCursor(signer: signer, backend: backendOptions)
        try await checkRestoredPeer(backend: backendOptions)
        try await checkIdentityRoutes(backend: backendOptions)
        try await checkStorageLayout(backend: backendOptions)
        try await checkAttachmentSettings(backend: backendOptions)
        try await checkAttachmentFlow(backend: backendOptions)
        try await checkAttachmentFailures(backend: backendOptions)
        try await checkAttachmentRecords(backend: backendOptions)
        try await checkAttachmentEnd(backend: backendOptions)
        let host = try await SDKClient.create(signer: signer, options: options)
        let client = host
        do {
            // Uppercase hex decodes, so only ID validation rejects it.
            _ = try await client.conversations().getMessageById(id: String(repeating: "AB", count: 32))
            throw ConformanceFailure("malformed ID was accepted")
        } catch let XmtpError.InvalidArgument(details) {
            precondition(details.code == "InvalidArgument")
            precondition(details.category == .input)
            precondition(!details.retryable)
        }
        let inboxId = client.inboxId()
        guard let storagePath = try await host.storage().path(),
              FileManager.default.fileExists(atPath: storagePath)
        else { throw ConformanceFailure("storage path does not name the database file") }
        let group = try await client.conversations().createGroup(members: [InboxId](), options: nil)
        var typedSends = 0
        for sample in codecSamples {
            let id: MessageId
            switch sample.value {
            case let .text(text): id = try await group.sendText(text: text, options: nil)
            case let .markdown(markdown): id = try await group.sendMarkdown(markdown: markdown, options: nil)
            case let .reaction(reference, inboxId, reaction): id = try await group.sendReaction(reference: reference, referenceInboxId: inboxId, reaction: reaction, options: nil)
            case let .reply(reference, inboxId, content): id = try await group.sendReply(reference: reference, referenceInboxId: inboxId, content: content, options: nil)
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
            guard let wire = try await client.conversations().getMessageById(id: id),
                  sameEncoded(wire.encoded, sample.expected)
            else { throw ConformanceFailure("typed send bytes differ from codec") }
            typedSends += 1
        }
        guard typedSends == 12 else { throw ConformanceFailure("missing typed send cases") }
        print("Swift P69: typed send bytes match all 12 public codecs")
        let sentId = try await group.sendText(text: "conformance message", options: nil)
        let sent = try await group.messages(options: nil).first { $0.id == sentId }
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
            identity: await signer.identity(), options: options, inboxId: inboxId
        )
        let reopened = reopenedHost
        precondition(reopened.inboxId() == inboxId)
        let bundleIdentifier = Bundle.main.bundleIdentifier!
        let appFolder = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent(bundleIdentifier)
        try? FileManager.default.removeItem(at: appFolder)
        defer { try? FileManager.default.removeItem(at: appFolder) }
        do {
            _ = try await SDKClient.build(
                identity: await signer.identity(),
                options: ClientOptions(
                    backend: options.backend,
                    storage: StorageOptions(location: .default),
                    deviceSync: false
                ), inboxId: inboxId
            )
            throw ConformanceFailure("build opened a database with no identity")
        } catch XmtpError.IdentityNotFound {}
        let defaultFolder = appFolder.appendingPathComponent("xmtp")
        let defaultFiles = FileManager.default.enumerator(atPath: defaultFolder.path)?.allObjects as? [String] ?? []
        guard !defaultFiles.contains(where: { $0.hasSuffix(".db3") }) else {
            throw ConformanceFailure("build created a new database")
        }
        let defaultHost = try await SDKClient.create(
            signer: TestSigner(),
            options: ClientOptions(
                backend: options.backend,
                storage: StorageOptions(location: .default),
                deviceSync: false
            )
        )
        let expectedDefaultPath = try defaultFolder
            .appendingPathComponent(deploymentComponent(defaultHost.serverConfiguration().identifier))
            .appendingPathComponent(defaultHost.inboxId())
            .appendingPathComponent("xmtp.db3").path
        guard try await defaultHost.storage().path() == expectedDefaultPath,
              FileManager.default.fileExists(atPath: expectedDefaultPath)
        else { throw ConformanceFailure("default storage path is incorrect") }
        try await defaultHost.end()
        try FileManager.default.removeItem(at: appFolder)
        var orphan: Message!
        weak var weakHost: SDKClient?
        do {
            let shortLived = try await SDKClient.build(
                identity: await signer.identity(), options: options, inboxId: inboxId
            )
            weakHost = shortLived
            let shortGroup = try await shortLived.conversations().createGroup(members: [InboxId](), options: nil)
            let orphanId = try await shortGroup.sendText(text: "weak owner", options: nil)
            orphan = try await shortGroup.messages(options: nil).first { $0.id == orphanId }
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

        let reopenedGroup = try await reopened.conversations().createGroup(members: [InboxId](), options: nil)
        let reader = try await reopenedGroup.messageReader()
        let liveId = try await reopenedGroup.sendText(text: "durable stream", options: nil)
        let first = try await reader.next()
        precondition(first?.id == liveId)
        try await reader.end()
        let replay = try await reopenedGroup.messageReader()
        let repeated = try await replay.next()
        precondition(repeated?.id == liveId)
        let pending = Task { try await replay.next() }
        try await Task.sleep(for: .milliseconds(50))
        pending.cancel()
        try await replay.end()
        _ = try? await pending.value
        let stream = try await reopenedHost.messages(in: reopenedGroup)
        let adapterId = try await reopenedGroup.sendText(text: "adapter stream", options: nil)
        let iterator = stream.makeAsyncIterator()
        let fromAdapter = try await iterator.next()
        precondition(fromAdapter?.id == adapterId)
        let idle = Task { try await iterator.next() }
        try await Task.sleep(for: .milliseconds(50))
        idle.cancel()
        _ = try? await idle.value
        let protocolGroup = try await reopened.conversations().createGroup(members: [InboxId](), options: nil)
        let firstId = try await protocolGroup.sendText(text: "ack on request", options: nil)
        do {
            let protocolStream = try await reopenedHost.messages(in: protocolGroup)
            for try await value in protocolStream {
                precondition(value.id == firstId)
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
        precondition(replayed?.id == firstId, "adapter prefetched and acknowledged a value")
        try await reread.end()
        let secondId = try await protocolGroup.sendText(text: "second request", options: nil)
        do {
            let protocolStream = try await reopenedHost.messages(in: protocolGroup)
            var protocolIterator: SDKMessageStream.Iterator? = protocolStream.makeAsyncIterator()
            let firstAgain = try await protocolIterator?.next()
            precondition(firstAgain?.id == firstId)
            let second = try await protocolIterator?.next()
            precondition(second?.id == secondId)
            protocolIterator = nil
        }
        try await Task.sleep(for: .milliseconds(100))
        let afterAck = try await protocolGroup.messageReader()
        let remaining = try await afterAck.next()
        precondition(remaining?.id == secondId, "adapter did not acknowledge on next request")
        try await afterAck.end()
        let breakGroup = try await reopened.conversations().createGroup(members: [InboxId](), options: nil)
        let breakId = try await breakGroup.sendText(text: "close after break", options: nil)
        let (breakClose, breakCloseSignal) = AsyncStream<SDKStreamCloseReason>.makeStream()
        let retainedStream = try await reopenedHost.messages(
            in: breakGroup, onClose: { _ = breakCloseSignal.yield($0) }
        )
        for try await value in retainedStream {
            precondition(value.id == breakId)
            break
        }
        var breakCloseIterator = breakClose.makeAsyncIterator()
        let breakTimer = Task {
            try? await Task.sleep(for: .seconds(2))
            breakCloseSignal.finish()
        }
        guard let breakReason = await breakCloseIterator.next() else {
            throw ConformanceFailure("break did not close the stored message stream")
        }
        breakTimer.cancel()
        guard case .closed = breakReason else {
            throw ConformanceFailure("break reported a failed stream")
        }
        let (throwingClose, throwingCloseSignal) = AsyncStream<SDKStreamCloseReason>.makeStream()
        let throwingCloseStream = try await reopenedHost.messages(
            in: breakGroup,
            onClose: { reason in
                throwingCloseSignal.yield(reason)
                throw ConformanceFailure("close callback failed")
            }
        )
        for try await value in throwingCloseStream {
            precondition(value.id == breakId)
            break
        }
        var throwingCloseIterator = throwingClose.makeAsyncIterator()
        let throwingCloseTimer = Task {
            try? await Task.sleep(for: .seconds(2))
            throwingCloseSignal.finish()
        }
        guard let throwingCloseReason = await throwingCloseIterator.next() else {
            throw ConformanceFailure("throwing close callback was not called")
        }
        throwingCloseTimer.cancel()
        guard case .closed = throwingCloseReason else {
            throw ConformanceFailure("throwing close callback received a failed reason")
        }
        let breakReplay = try await breakGroup.messageReader()
        guard try await breakReplay.next()?.id == breakId else {
            throw ConformanceFailure("break acknowledged the last message")
        }
        try await breakReplay.end()
        try await checkReaderAppError(owner: reopenedHost, group: breakGroup, messageId: breakId)
        try await checkCooperativeReaderOpeningCancellation()
        try await checkLateReaderOpeningCleanup(owner: reopenedHost, group: protocolGroup)
        try await checkLifecycleStartupBarrier()
        try await checkLifecycleFailedSuspendResume(overlap: false)
        try await checkLifecycleFailedSuspendResume(overlap: true)
        let reopenedReader = try await protocolGroup.messageReader()
        try await reopenedReader.end()
        // When iteration ends, the reader is already released: a replacement
        // reader on the same group opens at once. A slow end makes a
        // detached teardown lose this race every time.
        let reopenScope = try await reopened.conversations().createGroup(members: [InboxId](), options: nil)
        func slowEndStream(
            next: @escaping @Sendable () async throws -> Message?
        ) -> SDKMessageStream {
            SDKReaderStream(open: {
                let reader = try await reopenScope.messageReader()
                return StreamHandle(
                    owner: reopenedHost,
                    next: next,
                    end: {
                        try? await Task.sleep(for: .milliseconds(200))
                        try? await reader.end()
                    },
                    connectionState: { await reader.connectionState() },
                    connectionStateChanged: { try await reader.connectionStateChanged(previous: $0) }
                )
            }, onClose: nil, onConnectionStateChange: nil)
        }
        func reopenScopeAfter(_ path: String) async throws {
            do {
                let replacement = try await reopenScope.messageReader()
                try await replacement.end()
            } catch XmtpError.ConsumerOwned {
                throw ConformanceFailure("\(path) ended iteration before the reader was released")
            }
        }
        guard try await slowEndStream(next: { nil }).makeAsyncIterator().next() == nil else {
            throw ConformanceFailure("ended stream delivered a message")
        }
        try await reopenScopeAfter("end of stream")
        do {
            _ = try await slowEndStream(next: { throw ConformanceFailure("read failed") })
                .makeAsyncIterator().next()
            throw ConformanceFailure("failed read delivered a message")
        } catch let failure as ConformanceFailure where failure.errorDescription == "read failed" {}
        try await reopenScopeAfter("read failure")
        let cancelledRead = Task {
            try await slowEndStream(next: {
                try await Task.sleep(for: .seconds(10))
                return nil
            }).makeAsyncIterator().next()
        }
        try await Task.sleep(for: .milliseconds(100))
        cancelledRead.cancel()
        _ = try? await cancelledRead.value
        try await reopenScopeAfter("cancellation")
        print("Swift reader released before iteration ends passed")
        do {
            let conversationOpen = TestFlag()
            SDKClient.conversationReaderOpeningForTest = {
                try? await Task.sleep(for: .milliseconds(300))
            }
            SDKClient.conversationReaderOpenedForTest = { _ in
                conversationOpen.set()
            }
            defer {
                SDKClient.conversationReaderOpeningForTest = nil
                SDKClient.conversationReaderOpenedForTest = nil
            }
            let conversationStream = try await reopenedHost.conversationStream()
            let conversationIterator = conversationStream.makeAsyncIterator()
            let conversationPending = Task { try await conversationIterator.next() }
            let conversationDeadline = Task {
                do { try await Task.sleep(for: .seconds(10)) }
                catch { return }
                conversationPending.cancel()
            }
            defer { conversationDeadline.cancel() }
            for _ in 0 ..< 1000 {
                if conversationOpen.value {
                    break
                }
                try await Task.sleep(for: .milliseconds(10))
            }
            guard conversationOpen.value else {
                conversationPending.cancel()
                throw ConformanceFailure("conversation reader was not open before group creation")
            }
            _ = try await reopened.conversations().createGroup(members: [InboxId](), options: nil)
            do {
                guard try await conversationPending.value != nil else {
                    throw ConformanceFailure("conversation stream missed a stored group")
                }
            } catch is CancellationError {
                throw ConformanceFailure("conversation stream did not deliver a group before the deadline")
            }
        }
        let monitorCalls = TestCounter()
        let (monitorClosed, monitorClosedSignal) = AsyncStream<Void>.makeStream()
        let fakeHandle = StreamHandle<Int>(
            owner: reopenedHost,
            next: {
                try await Task.sleep(for: .seconds(10))
                return nil
            },
            end: {},
            connectionState: { .connecting },
            connectionStateChanged: { _ in
                monitorCalls.increment()
                return .closed
            }
        )
        let fakeStream = SDKReaderStream<Int>(
            open: { fakeHandle },
            onClose: nil,
            onConnectionStateChange: { _, current in
                if current == .closed {
                    monitorClosedSignal.yield(())
                }
            }
        )
        let fakeRead = Task {
            let iterator = fakeStream.makeAsyncIterator()
            return try await iterator.next()
        }
        let monitorDeadline = Task {
            try? await Task.sleep(for: .seconds(5))
            monitorClosedSignal.finish()
        }
        var monitorClosedIterator = monitorClosed.makeAsyncIterator()
        guard await monitorClosedIterator.next() != nil else {
            fakeRead.cancel()
            throw ConformanceFailure("state monitor did not report Closed")
        }
        monitorDeadline.cancel()
        let callsAtClosed = monitorCalls.value
        try await Task.sleep(for: .milliseconds(100))
        fakeRead.cancel()
        _ = try? await fakeRead.value
        guard monitorCalls.value == callsAtClosed else {
            throw ConformanceFailure("state monitor kept reading after Closed")
        }
        // verifies: PROC-044
        // A reader opened on a connected connection reports Connected first.
        let (connectedStates, connectedStateSignal) = AsyncStream<ConnectionState>.makeStream()
        let connectedHandle = StreamHandle<Int>(
            owner: reopenedHost,
            next: {
                try await Task.sleep(for: .seconds(10))
                return nil
            },
            end: {},
            connectionState: { .connected },
            connectionStateChanged: { _ in
                try await Task.sleep(for: .seconds(10))
                return .closed
            }
        )
        let connectedStream = SDKReaderStream<Int>(
            open: { connectedHandle },
            onClose: nil,
            onConnectionStateChange: { _, current in
                connectedStateSignal.yield(current)
            }
        )
        let connectedRead = Task {
            let iterator = connectedStream.makeAsyncIterator()
            return try await iterator.next()
        }
        let connectedDeadline = Task {
            try? await Task.sleep(for: .seconds(5))
            connectedStateSignal.finish()
        }
        var connectedStateIterator = connectedStates.makeAsyncIterator()
        let firstConnectedState = await connectedStateIterator.next()
        connectedDeadline.cancel()
        connectedRead.cancel()
        _ = try? await connectedRead.value
        guard firstConnectedState == .connected else {
            throw ConformanceFailure(
                "connected reader first reported \(String(describing: firstConnectedState))"
            )
        }
        print("Swift scenario 7: durable stream and idle cancellation passed")

        let largeExpiry: Int64 = 9_007_199_254_740_993
        let credentialOptions = ClientOptions(
            backend: .options(options: BackendOptions(
                url: backendOptions.url,
                credential: Credential(name: nil, value: "Bearer initial", expiresAtSeconds: largeExpiry)
            )),
            storage: options.storage,
            deviceSync: false,
            workers: WorkerOptions(defaultIntervalNs: UInt64(largeExpiry))
        )
        let credentialHost = try await SDKClient.build(
            identity: await signer.identity(), options: credentialOptions, inboxId: inboxId
        )
        let savedOptions = credentialHost.options()
        guard savedOptions.workers?.defaultIntervalNs == UInt64(largeExpiry) else {
            throw ConformanceFailure("worker interval lost 64-bit precision")
        }
        // The options never return the backend token or the database key.
        guard case let .some(.options(options: savedBackend)) = savedOptions.backend,
              savedBackend.credential == nil,
              savedBackend.credentials == nil,
              savedOptions.storage.encryptionKey == nil
        else {
            throw ConformanceFailure("client options exposed a secret")
        }
        try await credentialHost.setCredential(credential: Credential(
            name: nil, value: "Bearer renewed", expiresAtSeconds: largeExpiry
        ))
        try await credentialHost.end()
        print("Swift scenario 3: credential update and 64-bit value passed")

        let snapshot = reopened.serverConfiguration()
        let fetched = try await fetchServerConfiguration(backend: .options(options: backendOptions))
        let staticBackend = try await Backend.connect(options: backendOptions)
        let staticIdentity = try await signer.identity()
        guard try await SDKClient.inboxId(for: staticIdentity, backend: .connected(backend: staticBackend)) == inboxId else {
            throw ConformanceFailure("backend-only inbox lookup returned a different ID")
        }
        guard try await SDKClient.canMessage([staticIdentity], backend: .connected(backend: staticBackend))["ethereum:\(staticIdentity.identifier)"] == true else {
            throw ConformanceFailure("backend-only canMessage did not find this inbox")
        }
        guard try await SDKClient.canMessage([staticIdentity], backend: .options(options: backendOptions))["ethereum:\(staticIdentity.identifier)"] == true else {
            throw ConformanceFailure("backend options canMessage did not find this inbox")
        }
        let sameText = "1111111111111111111111111111111111111111"
        let mixedIdentities = [
            PublicIdentity(identifier: sameText, kind: .ethereum),
            PublicIdentity(identifier: sameText, kind: .passkey),
            staticIdentity,
        ]
        func checkMixedCanMessage(_ result: [String: Bool]) throws {
            guard result.count == 3,
                  result["ethereum:\(sameText)"] == false,
                  result["passkey:\(sameText)"] == false,
                  result["ethereum:\(staticIdentity.identifier)"] == true
            else {
                throw ConformanceFailure("canMessage lost an identity kind or value")
            }
        }
        try checkMixedCanMessage(await reopened.canMessage(identities: mixedIdentities))
        try checkMixedCanMessage(await SDKClient.canMessage(mixedIdentities, backend: .connected(backend: staticBackend)))
        try checkMixedCanMessage(await SDKClient.canMessage(mixedIdentities, backend: .options(options: backendOptions)))
        do {
            _ = try await SDKClient.build(
                identity: staticIdentity,
                options: ClientOptions(backend: .connected(backend: staticBackend), storage: StorageOptions(location: .inMemory), deviceSync: false),
                inboxId: inboxId
            )
            throw ConformanceFailure("build opened a database with no identity")
        } catch XmtpError.IdentityNotFound {}
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
        let unsigned = unsignedHost
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
        try await metadataFields(options)
        print("Swift metadata fields and profiles passed")

        // verifies: IDENT-073, IDENT-074, IDENT-075, IDENT-076
        let preAuthLog = CallLog()
        var preAuthOptions = unsignedOptions
        preAuthOptions.handlers = ClientHandlers(preAuthenticate: RecordingPreAuthenticate(preAuthLog, fail: false))
        let preAuthenticated = try await SDKClient.create(
            signer: RecordingSigner(await generateLocalSigner(), preAuthLog),
            options: preAuthOptions
        )
        guard preAuthLog.calls.isEmpty else {
            throw ConformanceFailure("preAuthenticate ran before registration: \(preAuthLog.calls)")
        }
        try await preAuthenticated.register()
        guard preAuthLog.calls == ["pre-authenticate", "sign"] else {
            throw ConformanceFailure("preAuthenticate did not run before the signer: \(preAuthLog.calls)")
        }
        preAuthLog.removeAll()
        try await preAuthenticated.register()
        guard preAuthLog.calls.isEmpty else {
            throw ConformanceFailure("registered client ran preAuthenticate again: \(preAuthLog.calls)")
        }
        try await preAuthenticated.end()
        var failingOptions = unsignedOptions
        failingOptions.registration = RegistrationOptions(auto: true)
        failingOptions.handlers = ClientHandlers(preAuthenticate: RecordingPreAuthenticate(preAuthLog, fail: true))
        do {
            _ = try await SDKClient.create(
                signer: RecordingSigner(await generateLocalSigner(), preAuthLog),
                options: failingOptions
            )
            throw ConformanceFailure("failing preAuthenticate did not stop registration")
        } catch XmtpError.CallbackFailed {}
        guard preAuthLog.calls == ["pre-authenticate"] else {
            throw ConformanceFailure("failing preAuthenticate reached the signer: \(preAuthLog.calls)")
        }
        print("Swift host preAuthenticate runs before the signer")

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

        let family = try await reopened.conversations().createGroup(
            members: [InboxId](), options: CreateGroupOptions(name: "family group")
        )
        guard try await family.state().name == "family group",
              family.creatorInboxId() == inboxId,
              try await reopened.conversations().listGroups(options: nil).contains(where: { $0.id() == family.id() })
        else { throw ConformanceFailure("group options, immutable fields, or list failed") }
        print("Swift scenario 4: group options, state, and list passed")

        let parentId = try await family.sendText(text: "parent", options: nil)
        let reactionId = try await reopened.conversations().reactToMessage(
            id: parentId, reaction: Reaction(content: "👍", action: .added, schema: .unicode), options: nil
        )
        let replyId = try await reopened.conversations().replyToMessage(
            id: parentId, content: encodeText(text: "reply"), options: nil
        )
        guard case .text = try await reopened.decodeContent(encoded: encodeText(text: "decoded"))
        else { throw ConformanceFailure("standard content did not decode") }
        let familyMessages = try await family.messages(options: nil)
        guard let parent = familyMessages.first(where: { $0.id == parentId }),
              let reply = familyMessages.first(where: { $0.id == replyId }),
              parent.replyCount == 1, parent.reactions.first?.id == reactionId,
              reply.inReplyTo?.id == parentId
        else { throw ConformanceFailure("message reaction or reply edge was not materialized") }
        guard let reactionMessage = try await reopened.conversations().getMessageById(id: reactionId),
              case let .standard(.reaction(reference, referenceInboxId, reaction)) = reactionMessage.content,
              reference == parentId, referenceInboxId == inboxId, reaction.content == "👍"
        else { throw ConformanceFailure("reaction content lost its target") }
        var changedReaction = reactionMessage.data
        changedReaction.content = .reaction(
            reference: reactionId, referenceInboxId: inboxId,
            reaction: Reaction(content: "👍", action: .added, schema: .unicode)
        )
        guard reactionMessage != Message(data: changedReaction) else {
            throw ConformanceFailure("reaction target did not affect message equality")
        }
        let sameParent = try await reopened.conversations().getMessageById(id: parentId)
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
            identity: await signer.identity(), options: options, inboxId: inboxId, codecs: [codec]
        )
        let withoutCodec = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxId: inboxId
        )
        let slashHost = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxId: inboxId, codecs: [SlashCodec()]
        )
        let colliding = EncodedContent(
            type: ContentTypeId(authorityId: "example.org/a", typeId: "b", versionMajor: 1, versionMinor: 0),
            content: Data([1])
        )
        guard case .unknown = slashHost.decodeCustom(colliding, rawBytes: Data()) else {
            throw ConformanceFailure("codec key collision selected the wrong codec")
        }
        try await slashHost.end()
        let customId = try await family.send(encoded: codec.encode("codec value"), options: nil)
        guard let decoded = try await withCodec.conversations().getMessageById(id: customId),
              let undecoded = try await withoutCodec.conversations().getMessageById(id: customId)
        else { throw ConformanceFailure("custom message was not found") }
        guard case let .custom(_, _, value, nil) = decoded.content, value as? String == "codec value",
              case .unknown = undecoded.content
        else { throw ConformanceFailure("custom codec leaked between clients") }
        let customReplyId = try await withCodec.conversations().replyToMessage(
            id: customId, content: codec.encode("reply codec value"), options: nil
        )
        guard let customReply = try await withCodec.conversations().getMessageById(id: customReplyId),
              case let .some(.custom(_, _, value, nil)) = customReply.replyContent,
              value as? String == "reply codec value"
        else { throw ConformanceFailure("reply body custom codec did not run") }
        let failingHost = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxId: inboxId, codecs: [FailingCodec()]
        )
        guard let failed = try await failingHost.conversations().getMessageById(id: customId),
              case let .custom(_, _, nil, error) = failed.content, error != nil
        else { throw ConformanceFailure("throwing custom codec was not recorded") }
        guard let failedReply = try await failingHost.conversations().getMessageById(id: customReplyId)
        else { throw ConformanceFailure("failed custom reply missing") }
        try checkRetainedContent(failed, nestedFailure: failedReply)
        print("Swift retained_content_details passed")
        // verifies: PROC-045
        let failedStreamGroup = try await failingHost.conversations().createGroup(members: [InboxId](), options: nil)
        let badStreamId = try await failedStreamGroup.send(encoded: codec.encode("stream codec error"), options: nil)
        let nextStreamId = try await failedStreamGroup.sendText(text: "after codec error", options: nil)
        do {
            let stream = try await failingHost.messages(in: failedStreamGroup)
            let iterator = stream.makeAsyncIterator()
            guard let bad = try await iterator.next(), bad.id == badStreamId,
                  case let .custom(_, raw, nil, error?) = bad.content,
                  !raw.isEmpty, error.code == "CodecDecodeFailed", error.category == .callback,
                  let next = try await iterator.next(), next.id == nextStreamId,
                  case .standard(.text("after codec error")) = next.content
            else { throw ConformanceFailure("codec failure stopped the Swift stream") }
        }
        print("Swift codec_failure_keeps_stream_open passed")
        try await failingHost.end()
        print("Swift codec_scoped_to_client passed")
        let typedParent = try await customCodecPolicyAndIsolation(group: family, receiver: withoutCodec)
        print("Swift custom_codec_policy_and_isolation passed")
        try await codecPolicyFailureNeverPublishes(group: family, parent: typedParent)
        print("Swift codec_policy_failure_never_publishes passed")
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

        // verifies: EVENT-014
        // verifies: EVENT-050
        // verifies: EVENT-053
        let eventFilter = EventFilter(
            kinds: [.conversationJoined], groupIds: nil,
            contentTypes: nil, referencesOwnMessages: false
        )
        var eventReader: SDKEventStream.Iterator? = try await reopened.events(eventFilter).makeAsyncIterator()
        let eventSignal = EventSignal()
        let listenerId = try await reopenedHost.startListener(eventFilter) { _ in
            await eventSignal.mark()
        }
        _ = try await reopened.conversations().createGroup(members: [InboxId](), options: nil)
        guard try await eventReader?.next() != nil else {
            throw ConformanceFailure("event reader ended before event")
        }
        try await eventSignal.wait()
        await reopenedHost.stopListener(listenerId)
        eventReader = nil
        print("Swift scenario 8: event reader and listener passed")

        // verifies: EVENT-053
        let startPause = EventStartPause()
        await EventStartHookForTest.shared.set {
            await startPause.hold()
        }
        let lateCalls = EventSignal()
        let delayedId = try await reopenedHost.startListener(eventFilter) { _ in
            await lateCalls.mark()
        }
        _ = try await reopened.conversations().createGroup(members: [InboxId](), options: nil)
        try await startPause.waitUntilEntered()
        await reopenedHost.stopListener(delayedId)
        await startPause.release()
        await EventStartHookForTest.shared.set(nil)
        try await Task.sleep(nanoseconds: 100_000_000)
        guard !(await lateCalls.hasRun()) else {
            throw ConformanceFailure("callback started after stop returned")
        }
        print("Swift delayed listener stop passed")

        try await reopenedHost.end()
    }
}
