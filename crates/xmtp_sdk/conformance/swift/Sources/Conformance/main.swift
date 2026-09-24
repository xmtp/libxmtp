import Foundation
@testable import XmtpSdk

struct ConformanceFailure: LocalizedError {
    let errorDescription: String?

    init(_ check: String) {
        errorDescription = check
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

@main
struct Conformance {
    static func main() async throws {
        precondition(sdkVersion().hasPrefix("1.12.0"))
        let messageID = try MessageID.fromString(String(repeating: "a", count: 64))
        precondition(messageID.description.count == 64)
        print("Swift scenario 1: load, checksums, version passed")

        let signer = TestSigner()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("xmtp-sdk-conformance-\(UUID().uuidString)")
        let options = ClientOptions(
            backend: BackendOptions(url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"]!),
            storage: StorageOptions(location: .directory(directory.path)),
            deviceSync: false
        )
        let host = try await SDKClient.create(signer: signer, options: options)
        let client = host.raw
        let inboxID = client.inboxID()
        let group = try await client.conversations().createGroup(members: [])
        let sentID = try await group.sendText(text: "conformance message")
        let sent = try await group.messages().first { $0.id == sentID }
        precondition(sent != nil)
        let owningClient = try sent?.client()
        precondition(owningClient === host)
        try await host.end()
        do {
            _ = try sent?.client()
            preconditionFailure("ended client remained in the registry")
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
            let shortGroup = try await shortLived.raw.conversations().createGroup(members: [])
            let orphanID = try await shortGroup.sendText(text: "weak owner")
            orphan = try await shortGroup.messages().first { $0.id == orphanID }
        }
        precondition(weakHost == nil, "the registry kept the host client alive")
        do {
            _ = try orphan.client()
            preconditionFailure("released client remained in the registry")
        } catch XmtpError.ClientClosed {}
        print("Swift scenario 2: create, reopen, end passed")

        let reopenedGroup = try await reopened.conversations().createGroup(members: [])
        let reader = try await reopenedGroup.messageReader()
        let liveID = try await reopenedGroup.sendText(text: "durable stream")
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
        let adapterID = try await reopenedGroup.sendText(text: "adapter stream")
        let iterator = stream.makeAsyncIterator()
        let fromAdapter = try await iterator.next()
        precondition(fromAdapter?.id == adapterID)
        let idle = Task { try await iterator.next() }
        try await Task.sleep(for: .milliseconds(50))
        idle.cancel()
        _ = try? await idle.value
        let protocolGroup = try await reopened.conversations().createGroup(members: [])
        let firstID = try await protocolGroup.sendText(text: "ack on request")
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
        let secondID = try await protocolGroup.sendText(text: "second request")
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
            backend: BackendOptions(
                url: options.backend.url,
                credential: Credential(name: nil, value: "Bearer initial", expiresAtSeconds: largeExpiry)
            ),
            storage: StorageOptions(location: .inMemory),
            deviceSync: false
        )
        let credentialHost = try await SDKClient.build(
            identity: await signer.identity(), options: credentialOptions, inboxID: inboxID
        )
        guard credentialHost.raw.options().backend.credential?.expiresAtSeconds == largeExpiry else {
            throw ConformanceFailure("credential expiry lost 64-bit precision")
        }
        try await credentialHost.raw.setCredential(credential: Credential(
            name: nil, value: "Bearer renewed", expiresAtSeconds: largeExpiry
        ))
        try await credentialHost.end()
        print("Swift scenario 3: credential update and 64-bit value passed")

        let snapshot = reopened.serverConfiguration()
        let fetched = try await fetchServerConfiguration(options: options.backend)
        let staticBackend = try await Backend.connect(options: options.backend)
        let staticIdentity = try await signer.identity()
        guard try await SDKClient.inboxID(for: staticIdentity, backend: staticBackend) == inboxID else {
            throw ConformanceFailure("backend-only inbox lookup returned a different ID")
        }
        guard try await SDKClient.canMessage([staticIdentity], backend: staticBackend).first?.canMessage == true else {
            throw ConformanceFailure("backend-only canMessage did not find this inbox")
        }
        guard snapshot.identifier == fetched.identifier else {
            throw ConformanceFailure("configuration fetch returned a different deployment")
        }
        let refreshed = try await reopened.refreshServerConfiguration()
        guard refreshed.identifier == snapshot.identifier else {
            throw ConformanceFailure("configuration refresh returned a different deployment")
        }
        do {
            _ = try await fetchServerConfiguration(options: BackendOptions(url: "http://127.0.0.1:1"))
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

        try await reopenedHost.end()
    }
}
