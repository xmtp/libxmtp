import Foundation
@testable import XmtpSdk

/// A client whose files live under `root`, allowed to reach loopback storage.
private func fileOptions(
    _ backend: BackendOptions,
    _ root: URL,
    _ attachments: AttachmentOptions = AttachmentOptions(allowPrivateNetwork: true)
) -> ClientOptions {
    ClientOptions(
        backend: .options(options: backend),
        storage: StorageOptions(location: .directory(directory: root.path)),
        deviceSync: false,
        attachments: attachments
    )
}

private func temporaryRoot() throws -> URL {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent("xmtp-sdk-atch-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
}

private func bytesSource(_ text: String) -> AttachmentSource {
    .bytes(bytes: Data(text.utf8), filename: "note.txt", mimeType: "text/plain")
}

private func attachmentsDir(_ databasePath: String?) throws -> URL {
    guard let databasePath else { throw ConformanceFailure("the client has no database path") }
    return URL(fileURLWithPath: databasePath).deletingLastPathComponent().appendingPathComponent("attachments")
}

private func readText(_ path: String) throws -> String {
    try String(contentsOfFile: path, encoding: .utf8)
}

/// The path of a file inside `directory`, relative to it.
private func relative(_ path: String, to directory: URL) -> String? {
    let prefix = directory.path + "/"
    return path.hasPrefix(prefix) ? String(path.dropFirst(prefix.count)) : nil
}

/// Storage and server configuration fields, including 64-bit values.
func checkAttachmentSettings(backend: BackendOptions) async throws {
    let offered = AttachmentsConfiguration(
        baseUrl: ProcessInfo.processInfo.environment["XMTP_S3_BASE_URL"] ?? "http://127.0.0.1:9067/attachments",
        maxUploadBytes: 104_857_600,
        retentionSeconds: 0
    )
    let fetched = try await SDKClient.fetchServerConfiguration(backend: .options(options: backend))
    guard fetched.attachments == offered else {
        throw ConformanceFailure("server attachments \(String(describing: fetched.attachments))")
    }
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let large = UInt64.max - 1
    let settings = AttachmentOptions(
        maxDownloadBytes: large, maxPendingAgeSeconds: large - 2, allowPrivateNetwork: true
    )
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: fileOptions(backend, root, settings))
    guard client.serverConfiguration().attachments == offered,
          client.options().attachments == settings,
          client.attachments().offered()
    else { throw ConformanceFailure("client attachment settings changed") }
    try await client.end()
    print("Swift attachments: configuration and 64-bit options")
}

/// Create, send, upload, reopen, resume, and download on another client.
func checkAttachmentFlow(backend: BackendOptions) async throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let signer = await generateLocalSigner()
    let senderOptions = fileOptions(backend, root.appendingPathComponent("sender"))
    let sender = try await SDKClient.create(signer: signer, options: senderOptions)
    let receiver = try await SDKClient.create(
        signer: generateLocalSigner(), options: fileOptions(backend, root.appendingPathComponent("receiver"))
    )
    let attachments = sender.attachments()
    let events = try await EventQueue(sender)
    // The receiver's reader sees none of the sender's events.
    let receiverEvents = try await EventQueue(receiver)

    let content = "attachment bytes"
    let fromBytes = try await attachments.create(source: bytesSource(content))
    let remote = fromBytes.remoteAttachment()
    let bytesPath = try await fromBytes.localPath()
    guard try await fromBytes.status() == .waiting,
          try readText(bytesPath) == content,
          try await attachments.localPath(remote: remote) == bytesPath
    else { throw ConformanceFailure("a created attachment is not waiting at its path") }
    // The SDK copies a path source at create, so moving it changes nothing.
    let source = root.appendingPathComponent("photo.bin")
    try Data("path bytes".utf8).write(to: source)
    let fromPath = try await attachments.create(
        source: .path(path: source.path, filename: nil, mimeType: "application/octet-stream")
    )
    try FileManager.default.moveItem(at: source, to: root.appendingPathComponent("moved.bin"))
    let pathRemote = fromPath.remoteAttachment()
    let copiedPath = try await fromPath.localPath()
    guard try readText(copiedPath) == "path bytes" else {
        throw ConformanceFailure("the path source was not copied at create")
    }
    guard try await events.drain(sender).isEmpty else { throw ConformanceFailure("create sent an event") }

    // The record is complete before any upload, so the app sends it first.
    let dm = try await sender.conversations().createDm(peer: receiver.inboxId(), options: nil)
    let sent = try await dm.sendRemoteAttachment(attachment: remote, options: nil)
    // Concurrent uploads of one attachment share one transfer.
    try await withThrowingTaskGroup(of: Void.self) { group in
        for _ in 0 ..< 2 {
            group.addTask { try await fromBytes.upload() }
        }
        try await group.waitForAll()
    }
    guard try await fromBytes.status() == .complete else { throw ConformanceFailure("the upload is not complete") }
    let uploaded = try await events.drain(sender)
    guard uploaded.map(\.kind) == [.attachmentUploadStarted, .attachmentUploadCompleted],
          uploaded[0].with(.attachmentUploadCompleted) == uploaded[1],
          uploaded[0].url == remote.url,
          uploaded[0].contentDigest == remote.contentDigest
    else { throw ConformanceFailure("expected one shared upload, got \(uploaded)") }
    guard try await receiverEvents.drain(receiver).isEmpty else {
        throw ConformanceFailure("the receiver saw the sender's events")
    }
    try await sender.end()
    // Held values stay readable after end; calls fail closed.
    guard attachments.offered(), fromBytes.remoteAttachment() == remote else {
        throw ConformanceFailure("held values changed after end")
    }
    try await checkClientClosed { _ = try await fromPath.status() }
    try await checkClientClosed { _ = try await attachments.listPending() }

    // A reopened client lists the upload it did not finish and resumes it.
    let reopened = try await SDKClient.build(identity: signer.identity(), options: senderOptions)
    let resuming = reopened.attachments()
    let listed = try await resuming.listPending()
    guard listed.count == 1, listed[0].remoteAttachment().contentDigest == pathRemote.contentDigest else {
        throw ConformanceFailure("expected one pending upload, got \(listed.count)")
    }
    guard try await resuming.pending(remote: remote).status() == .complete else {
        throw ConformanceFailure("a finished upload is not complete after reopen")
    }
    let resumed = try await resuming.pending(remote: pathRemote)
    guard try await resumed.status() == .waiting else { throw ConformanceFailure("the unfinished upload is not waiting") }
    try await resumed.upload()
    guard try await resumed.status() == .complete, try await resuming.listPending().isEmpty else {
        throw ConformanceFailure("the resumed upload did not finish")
    }
    var unstaged = pathRemote
    unstaged.contentDigest = String(repeating: "00", count: 32)
    guard try await thrownFailure({ _ = try await resuming.pending(remote: unstaged) }) == failure(.stagedUnusable) else {
        throw ConformanceFailure("an unknown record did not fail as staged_unusable")
    }

    // The receiver derives the path of the record it was sent. An unreachable
    // URL shows that the path needs no request.
    _ = try await receiver.conversations().syncAll(consentStates: nil)
    guard let message = try await receiver.conversations().getMessageById(id: sent),
          case let .standard(.remoteAttachment(received)) = message.content
    else { throw ConformanceFailure("the sent attachment did not arrive as a remote attachment") }
    let receiving = receiver.attachments()
    let directory = try await attachmentsDir(receiver.storage().path())
    var unreachable = received
    unreachable.url = "http://127.0.0.1:1/file"
    let derived = try await receiving.localPath(remote: unreachable)
    guard relative(derived, to: directory) != nil else {
        throw ConformanceFailure("a derived path is outside the attachments directory")
    }
    let expectedPath = try await receiving.localPath(remote: received)
    guard !FileManager.default.fileExists(atPath: expectedPath) else {
        throw ConformanceFailure("a path was written before download")
    }

    // Download events arrive in order; a filtered reader sees only its kind.
    let deletedOnly = try await EventQueue(receiver, attachmentFilter([.attachmentDeleted]))
    let downloaded = try await receiving.download(remote: received)
    guard downloaded == DownloadedAttachment(path: expectedPath, mimeType: "text/plain", filename: "note.txt"),
          try readText(downloaded.path) == content
    else { throw ConformanceFailure("downloaded \(downloaded)") }
    let pathDownload = try await receiving.download(remote: pathRemote)
    guard try readText(pathDownload.path) == "path bytes" else { throw ConformanceFailure("the path download differs") }
    let local = try await receiving.listLocal().map(\.path).sorted()
    let downloadedPaths = [downloaded.path, pathDownload.path].compactMap { relative($0, to: directory) }.sorted()
    guard local == downloadedPaths, downloadedPaths.count == 2 else {
        throw ConformanceFailure("local attachments \(local)")
    }
    try await receiving.deleteLocal(remote: received)
    guard try await receiving.listLocal().count == 1, !FileManager.default.fileExists(atPath: downloaded.path) else {
        throw ConformanceFailure("delete left the download")
    }
    let downloads = try await receiverEvents.drain(receiver)
    let downloadKinds: [EventKind] = [
        .attachmentDownloadStarted, .attachmentDownloadCompleted,
        .attachmentDownloadStarted, .attachmentDownloadCompleted,
        .attachmentDeleted,
    ]
    let downloadDigests = [
        received.contentDigest, received.contentDigest,
        pathRemote.contentDigest, pathRemote.contentDigest,
        received.contentDigest,
    ]
    guard downloads.map(\.kind) == downloadKinds,
          downloads.map(\.contentDigest) == downloadDigests,
          downloads[0].url == received.url,
          downloads[0].attachmentKey != downloads[2].attachmentKey,
          downloads[4].with(downloads[0].kind) == downloads[0]
    else { throw ConformanceFailure("download events \(downloads)") }
    guard try await deletedOnly.drain(receiver) == [downloads[4]] else {
        throw ConformanceFailure("the filtered reader saw another kind")
    }
    try await reopened.end()
    try await receiver.end()
    print("Swift attachments: upload, reopen, resume, download, and delete")
}

/// Failed uploads and downloads carry one record in errors and status.
func checkAttachmentFailures(backend: BackendOptions) async throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: fileOptions(backend, root))
    let attachments = client.attachments()
    let events = try await EventQueue(client)
    let staged = try await attachmentsDir(client.storage().path()).appendingPathComponent(".staged")

    // Missing staged data fails the upload.
    let pending = try await attachments.create(source: bytesSource("staged"))
    let remote = pending.remoteAttachment()
    let ciphertext = try Data(contentsOf: staged.appendingPathComponent(remote.contentDigest))
    try FileManager.default.removeItem(at: staged.appendingPathComponent(remote.contentDigest))
    let other = try await attachments.create(source: bytesSource("other"))
    let otherRemote = other.remoteAttachment()
    let otherCiphertext = try Data(contentsOf: staged.appendingPathComponent(otherRemote.contentDigest))
    let thrown = try await thrownFailure { try await pending.upload() }
    guard thrown == failure(.stagedUnusable), try await pending.status() == .failed(thrown) else {
        throw ConformanceFailure("thrown \(thrown)")
    }
    let failed = try await events.drain(client)
    guard failed.map(\.kind) == [.attachmentUploadStarted, .attachmentUploadFailed],
          failed[1] == failed[0].with(.attachmentUploadFailed, cause: "staged_unusable"),
          failed[1].contentDigest == remote.contentDigest
    else { throw ConformanceFailure("upload events \(failed)") }
    // A source the SDK cannot read fails create.
    let missing = AttachmentSource.path(
        path: root.appendingPathComponent("missing.bin").path, filename: nil, mimeType: "application/octet-stream"
    )
    guard try await thrownFailure({ _ = try await attachments.create(source: missing) }).cause == .sourceUnreadable else {
        throw ConformanceFailure("a missing source did not fail as source_unreadable")
    }

    // The creating client holds the plaintext, so another client downloads.
    let downloader = try await SDKClient.create(
        signer: generateLocalSigner(), options: fileOptions(backend, root.appendingPathComponent("downloader"))
    )
    let downloads = downloader.attachments()
    let downloadEvents = try await EventQueue(downloader)
    var unavailable = remote
    let store = try objectStore()
    unavailable.url = "\(store)/status/503"
    let (details, unavailableFailure) = try await thrownAttachment {
        _ = try await downloads.download(remote: unavailable)
    }
    guard unavailableFailure == failure(.httpStatus, httpStatus: 503),
          details.category == .network,
          details.retryable
    else { throw ConformanceFailure("failure \(unavailableFailure), \(details)") }
    // Another attachment's object decrypts and decodes, so only its digest
    // differs from the record.
    var substitute = otherRemote
    substitute.url = try servedObject(otherCiphertext)
    substitute.contentDigest = remote.contentDigest
    guard try await thrownFailure({ _ = try await downloads.download(remote: substitute) }).cause == .digestMismatch else {
        throw ConformanceFailure("a substituted object did not fail as digest_mismatch")
    }
    // A changed tag byte with a matching digest fails only the decryption.
    var tamperedBytes = ciphertext
    tamperedBytes[tamperedBytes.endIndex - 1] ^= 1
    var tampered = remote
    tampered.url = try servedObject(tamperedBytes)
    tampered.contentDigest = sha256Hex(tamperedBytes)
    guard try await thrownFailure({ _ = try await downloads.download(remote: tampered) }).cause == .decryptionFailed else {
        throw ConformanceFailure("a changed tag did not fail as decryption_failed")
    }
    var malformed = remote
    malformed.url = tampered.url
    malformed.secret = Data([7, 7, 7])
    guard try await thrownFailure({ _ = try await downloads.download(remote: malformed) }).cause == .malformed else {
        throw ConformanceFailure("a short secret did not fail as malformed")
    }
    let downloadFailures = try await downloadEvents.drain(downloader)
    let failedCauses = downloadFailures.filter { $0.kind == .attachmentDownloadFailed }.map(\.cause)
    guard failedCauses == ["http_status", "digest_mismatch", "decryption_failed"] else {
        throw ConformanceFailure("download events \(downloadFailures)")
    }
    try await downloader.end()
    try await client.end()
    print("Swift attachments: real failures carry one record")
}

/// Every transport discriminant and optional field. Rust owns the full policy table.
private let failureTable: [AttachmentFailure] = [
    failure(.notOffered),
    failure(.tooLarge),
    failure(.sourceUnreadable),
    failure(.localStorage),
    failure(.stagedUnusable),
    failure(.connectionBlocked),
    failure(.credential, credentialKind: .credentialRejected, missingScope: true),
    failure(.credential, credentialKind: .callbackFailed, retryable: true),
    failure(.credential, credentialKind: .exhausted),
    failure(.credential, credentialKind: .missingCredential),
    failure(.backendRejected),
    failure(.backendUnavailable),
    failure(.targetRejected, httpStatus: 403),
    failure(.network),
    failure(.insecureUrl),
    failure(.blockedAddress),
    failure(.tooManyRedirects),
    failure(.notFound, httpStatus: 404),
    failure(.httpStatus, httpStatus: 408),
    failure(.httpStatus, httpStatus: 429),
    failure(.httpStatus, httpStatus: 503),
    failure(.httpStatus, httpStatus: 403),
    failure(.malformed),
    failure(.digestMismatch),
    failure(.decryptionFailed),
    failure(.notAnAttachment),
    failure(.deleted),
]

/// A distinct number for each cause and kind. The switches have no default, so
/// a new case does not compile until the table covers it.
private func causeNumber(_ cause: AttachmentFailureCause) -> Int {
    switch cause {
    case .notOffered: 0
    case .tooLarge: 1
    case .sourceUnreadable: 2
    case .localStorage: 3
    case .stagedUnusable: 4
    case .connectionBlocked: 5
    case .credential: 6
    case .backendRejected: 7
    case .backendUnavailable: 8
    case .targetRejected: 9
    case .network: 10
    case .insecureUrl: 11
    case .blockedAddress: 12
    case .tooManyRedirects: 13
    case .notFound: 14
    case .httpStatus: 15
    case .malformed: 16
    case .digestMismatch: 17
    case .decryptionFailed: 18
    case .notAnAttachment: 19
    case .deleted: 20
    }
}

private func kindNumber(_ kind: CredentialFailureKind) -> Int {
    switch kind {
    case .credentialRejected: 0
    case .callbackFailed: 1
    case .exhausted: 2
    case .missingCredential: 3
    }
}

/// Every cause and credential kind, thrown and recorded, and no resend.
func checkAttachmentRecords(backend: BackendOptions) async throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let held = try await HeldTransfer.open()
    var relayedBackend = backend
    relayedBackend.url = held.backend
    let client = try await SDKClient.create(signer: generateLocalSigner(), options: fileOptions(relayedBackend, root))
    let attachments = client.attachments()
    for (index, recorded) in failureTable.enumerated() {
        let (details, thrown) = try await thrownAttachment {
            try await sdkConformanceAttachmentError(failure: recorded)
        }
        guard thrown == recorded else {
            throw ConformanceFailure("\(recorded.cause): thrown \(thrown), \(details)")
        }
        if recorded.cause == .credential {
            guard details.category == .callback, details.retryable == recorded.retryable else {
                throw ConformanceFailure("credential error details changed")
            }
        }
        let pending = try await attachments.create(source: bytesSource("record \(index)"))
        try await pending.sdkConformanceFail(failure: recorded)
        let status = try await pending.status()
        guard status == .failed(recorded) else { throw ConformanceFailure("status \(status)") }
    }
    let causes = Set(failureTable.map { causeNumber($0.cause) })
    let kinds = Set(failureTable.compactMap { $0.credentialKind.map(kindNumber) })
    guard causes == Set(0 ..< 21), kinds == Set(0 ..< 4) else {
        throw ConformanceFailure("the table misses a cause or credential kind")
    }

    // A terminal backend rejection is not sent again.
    try await held.command("release")
    let events = try await EventQueue(client)
    let rejected = try await attachments.create(source: bytesSource("rejected"))
    try await rejected.sdkConformanceFail(failure: failure(.backendRejected))
    for _ in 0 ..< 2 {
        guard try await thrownFailure({ try await rejected.upload() }) == failure(.backendRejected) else {
            throw ConformanceFailure("a rejected upload changed its failure")
        }
    }
    guard try await rejected.status() == .failed(failure(.backendRejected)),
          try await events.drain(client).isEmpty
    else { throw ConformanceFailure("a rejected upload was sent again") }
    try await held.checkCounts(puts: 0, grants: 0)
    try await client.end()
    print("Swift attachments: every cause and credential kind in both forms")
}

/// End waits for an operation in flight; later calls fail closed.
func checkAttachmentEnd(backend: BackendOptions) async throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let signer = await generateLocalSigner()
    let held = try await HeldTransfer.open()
    var relayedBackend = backend
    relayedBackend.url = held.backend
    let options = fileOptions(relayedBackend, root)
    let client = try await SDKClient.create(signer: signer, options: options)
    do {
        let attachments = client.attachments()
        let small = try await attachments.create(source: bytesSource("small"))
        let events = try await EventQueue(client, attachmentFilter([.attachmentUploadStarted]))
        let large = try await attachments.create(source: bytesSource("held upload"))
        let upload = Task { try await large.upload() }
        try await held.command("entered")
        guard case .attachmentUploadStarted = try await events.next() else {
            throw ConformanceFailure("the upload did not start")
        }
        // Swift's binding does not expose caller cancellation. Exercise end with
        // the actual transfer held at the server response instead.
        let ended = TestCounter()
        let ending = Task { try await client.end(); ended.increment() }
        guard try await events.ended() else { throw ConformanceFailure("the event reader outlived end") }
        guard ended.value == 0 else { throw ConformanceFailure("end released storage while PUT was held") }
        try await held.checkCounts(puts: 1, grants: 1)
        try await held.command("release")
        try await within(seconds: 30) { try await ending.value }
        try await within(seconds: 30) { try await upload.value }
        // Held values stay readable; calls fail closed.
        guard attachments.offered() else { throw ConformanceFailure("a held value changed after end") }
        let remote = large.remoteAttachment()
        try await checkClientClosed { _ = try await attachments.create(source: bytesSource("late")) }
        try await checkClientClosed { _ = try await attachments.localPath(remote: remote) }
        try await checkClientClosed { _ = try await attachments.listLocal() }
        try await checkClientClosed { _ = try await attachments.download(remote: remote) }
        try await checkClientClosed { _ = try await attachments.pending(remote: remote) }
        try await checkClientClosed { _ = try await attachments.listPending() }
        try await checkClientClosed { try await attachments.deleteLocal(remote: remote) }
        try await checkClientClosed { _ = try await large.localPath() }
        try await checkClientClosed { _ = try await large.status() }
        try await checkClientClosed { try await large.upload() }

        // The held wrappers do not keep the ended database open.
        let reopened = try await SDKClient.build(identity: signer.identity(), options: options)
        let resumed = reopened.attachments()
        guard try await resumed.pending(remote: remote).status() == .complete else {
            throw ConformanceFailure("the upload did not finish before end")
        }
        try await held.checkCounts(puts: 1, grants: 1)
        // A listener sees a deletion until it stops.
        let first = AttachmentSignal()
        let deletions = TestCounter()
        let listener = try await reopened.startListener(
            EventFilter(kinds: [.attachmentDeleted], groupIds: nil, contentTypes: nil, referencesOwnMessages: false)
        ) { _ in deletions.increment(); await first.mark() }
        let deleted = try await EventQueue(reopened, attachmentFilter([.attachmentDeleted]))
        try await resumed.deleteLocal(remote: remote)
        try await within { await first.wait() }
        guard deletions.value == 1 else { throw ConformanceFailure("the listener saw \(deletions.value) deletions") }
        let entered = AttachmentSignal()
        let release = AttachmentSignal()
        let finished = AttachmentSignal()
        await EventStartHookForTest.shared.set {
            await entered.mark()
            await release.wait()
        } finished: {
            await finished.mark()
        }
        do {
            try await resumed.deleteLocal(remote: small.remoteAttachment())
            try await within { await entered.wait() }
            await reopened.stopListener(listener)
            await release.mark()
            try await within { await finished.wait() }
            guard deletions.value == 1 else { throw ConformanceFailure("a stopped listener saw a deletion") }
        } catch {
            await release.mark()
            await EventStartHookForTest.shared.set(nil)
            throw error
        }
        await EventStartHookForTest.shared.set(nil)
        let removedPath = try await resumed.localPath(remote: remote)
        guard !FileManager.default.fileExists(atPath: removedPath) else {
            throw ConformanceFailure("delete left the attachment")
        }
        // The reader has both deletions, so a live listener had its turn.
        for expected in [remote, small.remoteAttachment()] {
            let next = try await deleted.next()
            guard case let .attachmentDeleted(attachment) = next, attachment.url == expected.url else {
                throw ConformanceFailure("not a deletion: \(next)")
            }
        }
        guard deletions.value == 1 else { throw ConformanceFailure("a stopped listener saw a deletion") }
        try await reopened.end()
        print("Swift attachments: end waits for an upload; calls fail closed")
    } catch {
        _ = try? await held.command("release")
        try? await client.end()
        throw error
    }
}
