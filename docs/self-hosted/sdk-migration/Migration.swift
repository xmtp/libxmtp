import Foundation
import XmtpSdk

public enum MigrationFailure: Error {
    case pathChanged
    case inboxChanged
}

/// End the old SDK client before calling this function.
public func exerciseMigration(
    signer: Signer, options: ClientOptions, dbPath: String, attachmentsDir: String
) async throws {
    let expectedPath = URL(fileURLWithPath: dbPath).standardizedFileURL.path
    var explicit = options
    explicit.storage.location = .explicit(dbPath: dbPath, attachmentsDir: attachmentsDir)
    let first = try await SDKClient.create(signer: signer, options: explicit)
    let identity = first.identity()
    let inboxId = first.inboxId()
    let openedPath = try await first.storage().path()
    try await first.end()
    guard let openedPath, URL(fileURLWithPath: openedPath).standardizedFileURL.path == expectedPath else { throw MigrationFailure.pathChanged }

    explicit.allowOffline = true
    let reopened = try await SDKClient.build(identity: identity, options: explicit)
    do {
        guard reopened.inboxId() == inboxId else { throw MigrationFailure.inboxChanged }
        guard let reopenedPath = try await reopened.storage().path(),
              URL(fileURLWithPath: reopenedPath).standardizedFileURL.path == expectedPath else { throw MigrationFailure.pathChanged }
    } catch {
        try await reopened.end()
        throw error
    }
    try await reopened.end()
}
