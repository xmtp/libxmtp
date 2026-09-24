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
            backend: BackendOptions(url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://127.0.0.1:9150"),
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
        precondition(owningClient === client)
        try await host.end()
        do {
            _ = try sent?.client()
            preconditionFailure("ended client remained in the registry")
        } catch SDKValueError.clientClosed {}
        let reopenedHost = try await SDKClient.build(
            identity: await signer.identity(), options: options, inboxID: inboxID
        )
        let reopened = reopenedHost.raw
        precondition(reopened.inboxID() == inboxID)
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
        var iterator = stream.makeAsyncIterator()
        let fromAdapter = try await iterator.next()
        precondition(fromAdapter?.id == adapterID)
        let idle = Task { try await iterator.next() }
        try await Task.sleep(for: .milliseconds(50))
        idle.cancel()
        _ = try? await idle.value
        try await reopenedHost.end()
        print("Swift scenario 7: durable stream and idle cancellation passed")
    }
}
