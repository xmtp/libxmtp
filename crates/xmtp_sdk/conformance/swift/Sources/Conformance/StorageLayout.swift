import Foundation
import XmtpSdk

private func directoryOptions(_ backend: BackendOptions, _ directory: URL, label: String? = nil) -> ClientOptions {
    ClientOptions(
        backend: .options(options: backend),
        storage: StorageOptions(location: .directory(directory: directory.path), label: label),
        deviceSync: false
    )
}

private func checkStorageLocation(_ action: () async throws -> Void) async throws {
    do {
        try await action()
    } catch let XmtpError.StorageLocation(details) {
        guard details.category == .storage, !details.retryable else {
            throw ConformanceFailure("storage location error \(details)")
        }
        return
    } catch {
        throw ConformanceFailure("expected a storage location error, got \(error)")
    }
    throw ConformanceFailure("expected a storage location error, got success")
}

/// Storage locations open the layouts they name, offline from their record.
func checkStorageLayout(backend: BackendOptions) async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent("xmtp-sdk-layout-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    // A labelled directory holds the store at its deployment path.
    let signer = await generateLocalSigner()
    let identity = try await signer.identity()
    let online = try await SDKClient.create(signer: signer, options: directoryOptions(backend, root, label: "phone"))
    let inboxId = online.inboxId()
    let expected = try root
        .appendingPathComponent("phone")
        .appendingPathComponent(deploymentComponent(online.serverConfiguration().identifier))
        .appendingPathComponent(inboxId)
        .appendingPathComponent("xmtp.db3").path
    guard try await online.storage().path() == expected, FileManager.default.fileExists(atPath: expected) else {
        throw ConformanceFailure("labelled storage path is not \(expected)")
    }
    try await online.end()

    var offlineOptions = directoryOptions(backend, root, label: "phone")
    offlineOptions.allowOffline = true
    let offline = try await SDKClient.build(identity: identity, options: offlineOptions, inboxId: inboxId)
    guard offline.inboxId() == inboxId, try await offline.storage().path() == expected else {
        throw ConformanceFailure("offline build opened another store")
    }
    try await offline.end()
    // The unlabelled root records no deployment, so an offline first start fails.
    var unrecorded = directoryOptions(backend, root)
    unrecorded.allowOffline = true
    try await checkStorageLocation {
        _ = try await SDKClient.build(identity: identity, options: unrecorded, inboxId: inboxId)
    }
    print("Swift storage layout: a labelled directory reopens offline")

    // Unsafe labels fail before any path.
    let unsafeRoot = root.appendingPathComponent("unsafe")
    for label in [".", "..", "bad/name", "bad\\name", "bad:name", "a\u{0}b"] {
        try await checkStorageLocation {
            _ = try await SDKClient.create(signer: signer, options: directoryOptions(backend, unsafeRoot, label: label))
        }
    }
    guard !FileManager.default.fileExists(atPath: unsafeRoot.path) else {
        throw ConformanceFailure("an unsafe label created its root")
    }
    print("Swift storage layout: unsafe labels fail before any path")

    // An explicit location opens the file the app chose.
    let explicitSigner = await generateLocalSigner()
    let dbPath = root.appendingPathComponent("chosen.sqlite").path
    var explicit = ClientOptions(
        backend: .options(options: backend),
        storage: StorageOptions(
            location: .explicit(dbPath: dbPath, attachmentsDir: root.appendingPathComponent("files").path)
        ),
        deviceSync: false
    )
    let chosen = try await SDKClient.create(signer: explicitSigner, options: explicit)
    let chosenInbox = chosen.inboxId()
    guard try await chosen.storage().path() == dbPath else { throw ConformanceFailure("explicit path moved") }
    try await chosen.end()
    explicit.allowOffline = true
    let reopened = try await SDKClient.build(identity: explicitSigner.identity(), options: explicit, inboxId: nil)
    guard reopened.inboxId() == chosenInbox, try await reopened.storage().path() == dbPath else {
        throw ConformanceFailure("offline explicit reopen opened another store")
    }
    try await reopened.end()
    print("Swift storage layout: an explicit location reopens offline")
}
