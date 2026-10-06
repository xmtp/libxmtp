import Foundation
@testable import XmtpSdk

@main
struct Conformance {
    static func main() async throws {
        setbuf(stdout, nil)
        if CommandLine.arguments.contains("--missing-bundle") {
            guard Bundle.main.bundleIdentifier == nil else {
                throw ConformanceFailure("bare executable unexpectedly has a bundle identifier")
            }
            do {
                _ = try await SDKClient.create(
                    signer: await generateLocalSigner(),
                    options: ClientOptions(storage: StorageOptions(location: .default))
                )
                throw ConformanceFailure("default storage accepted a missing bundle identifier")
            } catch XmtpError.StorageLocationRequired {}
            print("Swift missing bundle identifier rejected")
            return
        }
        // The remaining proofs need the conformance copy of the runtime. The
        // public behavior moved to sdks/ios/Tests.
        let backend = BackendOptions(url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"]!)
        let host = try await SDKClient.create(
            signer: await generateLocalSigner(),
            options: ClientOptions(
                backend: .options(options: backend),
                storage: StorageOptions(location: .inMemory),
                deviceSync: false
            )
        )
        let group = try await host.conversations().createGroup(members: [InboxId](), options: nil)
        let heldId = try await group.sendText(text: "held by the app", options: nil)
        try await checkReaderAppError(owner: host, group: group, messageId: heldId)
        try await checkCooperativeReaderOpeningCancellation()
        try await checkLateReaderOpeningCleanup(owner: host, group: group)
        try await checkReaderReleasedBeforeIterationEnds(owner: host)
        try await checkConversationReaderOpensBeforeDelivery(owner: host)
        try await checkConnectionStateMonitor(owner: host)
        try await checkDelayedListenerStop(owner: host)
        try await host.end()
        print("Swift runtime seam proofs passed")
    }
}
