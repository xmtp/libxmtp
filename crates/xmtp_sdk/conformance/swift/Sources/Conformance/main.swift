import Foundation
import XmtpSdk

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
        guard process.terminationStatus == 0 else { throw SDKValueError.invalidID }
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
        let defaultHost = try await SDKClient.build(
            identity: await signer.identity(),
            options: ClientOptions(
                backend: options.backend,
                storage: StorageOptions(location: .default),
                deviceSync: false
            ), inboxID: inboxID, appName: appName
        )
        let defaultFolder = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent(appName).appendingPathComponent("xmtp")
        let defaultFiles = try FileManager.default.contentsOfDirectory(atPath: defaultFolder.path)
        precondition(defaultFiles.contains { $0.hasSuffix(".db3") })
        try await defaultHost.end()
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
        let cancelledOpening = Task { try await reopenedHost.messages(in: protocolGroup) }
        cancelledOpening.cancel()
        _ = try? await cancelledOpening.value
        try await reopenedHost.end()
        print("Swift scenario 7: durable stream and idle cancellation passed")
    }
}
