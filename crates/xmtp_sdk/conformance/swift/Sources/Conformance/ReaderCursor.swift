import Foundation
import XmtpSdk

// verifies: PROC-033, PROC-034, PROC-050
func checkReaderCursor(signer: Signer, backend: BackendOptions) async throws {
    let path = FileManager.default.temporaryDirectory.appendingPathComponent("f3-cursor-\(UUID().uuidString).db").path
    let options = ClientOptions(backend: .options(options: backend), storage: StorageOptions(location: .path(path)), deviceSync: false)
    var host = try await SDKClient.create(signer: signer, options: options)
    let identity = try await signer.identity()
    let inbox = host.raw.inboxId()
    try await host.raw.conversations().sdkConformanceSeedDeliveryCursor()
    let group = try await host.raw.conversations().createGroup(members: [], options: nil)
    let groupId = group.id()
    let beginning = try await host.raw.conversations().beginningDeliveryCursor()
    let firstId = try await group.sendText(text: "large A")
    guard let first = try await group.messages(options: nil).first(where: { $0.id == firstId }), let cursor = first.deliveryCursor else { throw ConformanceFailure("cursor absent") }
    func sequence(_ cursor: String) throws -> UInt64 {
        guard cursor.hasPrefix("dc1_"), let bytes = Data(base64Encoded: String(cursor.dropFirst(4)).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")), bytes.count == 24 else { throw ConformanceFailure("invalid cursor") }
        return bytes.suffix(8).reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
    }
    let firstSequence = try sequence(cursor)
    precondition(firstSequence == 9_007_199_254_740_993)
    let lookup = try await host.raw.conversations().getMessageById(id: firstId)
    let refreshed = try await first.refresh()
    precondition(lookup?.deliveryCursor == cursor && refreshed?.deliveryCursor == cursor)
    precondition(Message(data: first.data) == first)
    precondition(Message(data: first.data).hashValue == first.hashValue)
    var changed = first.data
    changed.deliveryCursor = nil
    precondition(Message(data: changed) != first)
    let allClosed = AsyncStream<Void>.makeStream()
    let all = try await host.messages(options: MessageReaderOptions(conversationKind: .group, from: beginning), onClose: { _ in allClosed.continuation.finish() })
    var allCount = 0
    for try await item in all { allCount += 1; precondition(item.deliveryCursor == cursor); break }
    precondition(allCount == 1, "all reader ended before its first item")
    for await _ in allClosed.stream {}
    let namedClosed = AsyncStream<Void>.makeStream()
    let named = try await host.messages(in: group, onClose: { _ in namedClosed.continuation.finish() })
    var namedCount = 0
    for try await item in named { namedCount += 1; precondition(item.deliveryCursor == cursor); break }
    precondition(namedCount == 1, "named reader ended before its first item")
    for await _ in namedClosed.stream {}
    let secondId = try await group.sendText(text: "large B")
    let resume = try await group.messageReader(options: ConversationMessageReaderOptions(from: cursor))
    let second = try await resume.next()
    precondition(second?.id == secondId)
    let secondSequence = try sequence(second!.deliveryCursor!)
    precondition(secondSequence == 9_007_199_254_740_994)
    try await resume.end()
    let encoded = try TextCodec().encode("reply")
    let replyId = try await group.sendReply(reference: firstId, referenceInboxId: nil, content: encoded)
    let reply = try await host.raw.conversations().getMessageById(id: replyId)
    let parent = try await reply?.parent()
    precondition(parent?.deliveryCursor == cursor)
    let preparedId = try await group.prepareMessage(encoded: encoded)
    guard let pending = try await host.raw.conversations().getMessageById(id: preparedId) else { throw ConformanceFailure("optimistic message missing") }
    precondition(pending.id == preparedId)
    precondition(pending.deliveryCursor == nil)
    try await group.publishMessage(id: preparedId)
    let published = try await host.raw.conversations().getMessageById(id: preparedId)
    precondition(published?.deliveryCursor != nil)
    try await host.end()
    host = try await SDKClient.build(identity: identity, options: options, inboxId: inbox)
    guard case let .group(restored)? = try await host.raw.conversations().getById(id: groupId) else { throw ConformanceFailure("group missing") }
    let replayClosed = AsyncStream<Void>.makeStream()
    let replay = try await host.messages(in: restored, options: ConversationMessageReaderOptions(from: cursor), onClose: { _ in replayClosed.continuation.finish() })
    var replayCount = 0
    for try await repeated in replay {
        replayCount += 1
        precondition(repeated.id == secondId && repeated.deliveryCursor == second?.deliveryCursor)
        break
    }
    precondition(replayCount == 1, "replay reader ended before its first item")
    for await _ in replayClosed.stream {}
    try await host.end()
    print("Swift F3 exact large cursor, full message, equality, selection, and reopen passed")
}

// verifies: DMS-017, PROC-034, PROC-050
func checkRestoredPeer(backend: BackendOptions) async throws {
    let options = ClientOptions(backend: .options(options: backend), storage: StorageOptions(location: .inMemory), deviceSync: false)
    let a = try await SDKClient.create(signer: await generateLocalSigner(), options: options)
    let b = try await SDKClient.create(signer: await generateLocalSigner(), options: options)
    let c = try await SDKClient.create(signer: await generateLocalSigner(), options: options)
    let dm = try await a.raw.conversations().createDm(peer: b.raw.inboxId())
    let other = try await b.raw.conversations().createDm(peer: a.raw.inboxId())
    precondition(dm.id() != other.id())
    let peerA = try await dm.peerInboxId()
    let peerB = try await other.peerInboxId()
    precondition(peerA == b.raw.inboxId() && peerB == a.raw.inboxId())
    _ = try await a.raw.conversations().syncAll(consentStates: nil)
    let id = try await dm.sendText(text: "foreign restored DM")
    let key = Data(repeating: 9, count: 32)
    let archive = try await a.raw.archives().exportToBytes(keyBytes: key, options: ArchiveOptions(elements: [.messages]))
    try await c.raw.archives().importFromBytes(data: archive, keyBytes: key)
    guard case let .dm(restored)? = try await c.raw.conversations().getById(id: dm.id()) else { throw ConformanceFailure("DM missing") }
    let peer = try await restored.peerInboxId()
    precondition(peer == nil)
    let listed = try await c.raw.conversations().listDms(options: ListConversationsOptions(includeDuplicateDms: true))
    precondition(listed.count == 2)
    for item in listed { let peer = try await item.peerInboxId(); precondition(peer == nil) }
    let duplicates = try await restored.duplicateDms()
    precondition(duplicates.count == 1)
    let duplicatePeer = try await duplicates[0].peerInboxId()
    precondition(duplicatePeer == nil)
    let cursor = try await restored.messages(options: nil).first { $0.id == id }?.deliveryCursor
    precondition(cursor != nil)
    let beginning = try await c.raw.conversations().beginningDeliveryCursor()
    let closed = AsyncStream<Void>.makeStream()
    let stream = try await c.messages(in: restored, options: ConversationMessageReaderOptions(from: beginning), onClose: { _ in closed.continuation.finish() })
    var dmCount = 0
    for try await first in stream { dmCount += 1; precondition(first.id == id && first.deliveryCursor == cursor); break }
    precondition(dmCount == 1, "DM reader ended before its first item")
    for await _ in closed.stream {}
    let reader = try await restored.messageReader()
    let first = try await reader.next()
    precondition(first?.deliveryCursor == cursor)
    try await reader.end()
    do { _ = try await restored.messageReader(options: ConversationMessageReaderOptions(from: "invalid")); throw ConformanceFailure("invalid cursor accepted") } catch XmtpError.InvalidCursor {}
    let foreign = try await a.raw.conversations().beginningDeliveryCursor()
    do { _ = try await restored.messageReader(options: ConversationMessageReaderOptions(from: foreign)); throw ConformanceFailure("foreign cursor accepted") } catch XmtpError.ForeignCursor {}
    try await c.end()
    try await b.end()
    try await a.end()
    print("Swift F3 Restored peer lookup/list/duplicates and Dm selection passed")
}
