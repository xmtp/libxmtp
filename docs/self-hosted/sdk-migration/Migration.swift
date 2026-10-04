import Foundation
import XmtpSdk

public enum MigrationFailure: Error {
    case pathChanged
    case inboxChanged
}

/// End the old SDK client first. Use its stored 32-byte database key.
public func exerciseMigration(
    existingIdentity: PublicIdentity, options: ClientOptions, databaseKey: Data, dbPath: String, attachmentsDir: String
) async throws {
    let expectedPath = URL(fileURLWithPath: dbPath).standardizedFileURL.path
    var explicit = options
    explicit.storage.location = .explicit(dbPath: dbPath, attachmentsDir: attachmentsDir)
    explicit.storage.encryptionKey = databaseKey
    let first = try await SDKClient.build(identity: existingIdentity, options: explicit)
    let identity: PublicIdentity
    let inboxId: String
    let openedPath: String?
    do {
        identity = first.identity()
        inboxId = first.inboxId()
        openedPath = try await first.storage().path()
    } catch {
        try await endMigrationClient(first, preserving: error)
        throw error
    }
    try await endMigrationClient(first)
    try Task.checkCancellation()
    guard let openedPath, URL(fileURLWithPath: openedPath).standardizedFileURL.path == expectedPath else { throw MigrationFailure.pathChanged }

    explicit.allowOffline = true
    let reopened = try await SDKClient.build(identity: identity, options: explicit)
    do {
        guard reopened.inboxId() == inboxId else { throw MigrationFailure.inboxChanged }
        guard let reopenedPath = try await reopened.storage().path(),
              URL(fileURLWithPath: reopenedPath).standardizedFileURL.path == expectedPath else { throw MigrationFailure.pathChanged }
    } catch {
        try await endMigrationClient(reopened, preserving: error)
        throw error
    }
    try await endMigrationClient(reopened)
    try Task.checkCancellation()
}

private func endMigrationClient(_ client: SDKClient, preserving primaryError: Error? = nil) async throws {
    let cleanup = Task { try await client.end() }
    do {
        try await cleanup.value
    } catch {
        // Keep the error that started cleanup.
        throw primaryError ?? error
    }
}
