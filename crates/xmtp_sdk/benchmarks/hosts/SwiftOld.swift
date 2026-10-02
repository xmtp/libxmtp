import Foundation
import XMTPiOS

typealias BenchClient = Client
typealias BenchGroup = Group
typealias BenchLiveMessage = DecodedMessage
final class BenchSigner: SigningKey {
    let config: HostConfig; let key: String; let address: String; let delay: UInt64; let clock: CallbackClock
    init(_ config: HostConfig, _ key: String, _ address: String, _ delay: UInt64, _ clock: CallbackClock) {
        self.config = config; self.key = key; self.address = address; self.delay = delay; self.clock = clock
    }

    var identity: PublicIdentity {
        PublicIdentity(kind: .ethereum, identifier: address)
    }

    func sign(_ message: String) async throws -> SignedData {
        clock.mark(); if delay > 0 {
            try await Task.sleep(nanoseconds: delayNanoseconds(delay))
        }
        return try await SignedData(rawData: unhex(signerHelper(config, ["key": key, "text": message])["signature"]!))
    }
}

func benchOptions(_ config: HostConfig, _ path: String) throws -> ClientOptions {
    try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
    return ClientOptions(api: .init(env: .local, gatewayHost: config.backend_url),
                         codecs: [TextCodec(), AttachmentCodec(), ReplyCodec(), ReactionV2Codec()],
                         dbEncryptionKey: Data(repeating: 7, count: 32), dbDirectory: path, deviceSyncEnabled: false)
}

func benchCreate(_ config: HostConfig, _ key: String, _ address: String, _ path: String,
                 _ delay: UInt64, _ clock: CallbackClock) async throws -> BenchClient
{
    try await Client.create(account: BenchSigner(config, key, address, delay, clock), options: benchOptions(config, path))
}

func benchOpen(_ config: HostConfig, _ address: String, _ path: String, _ inbox: String) async throws -> BenchClient {
    try await Client.build(publicIdentity: PublicIdentity(kind: .ethereum, identifier: address), options: benchOptions(config, path), inboxId: inbox)
}

func benchClose(_ client: BenchClient) async throws {
    try client.dropLocalDatabaseConnection()
}

func benchInbox(_ client: BenchClient) -> String {
    client.inboxID
}

func benchSync(_ client: BenchClient) async throws {
    try await client.conversations.sync()
}

func benchGroupID(_ group: BenchGroup) -> String {
    group.id
}

func benchGroup(_ client: BenchClient, _ id: String) async throws -> BenchGroup {
    guard let group = try client.conversations.findGroup(groupId: id) else { throw BenchFailure(message: "Seeded group is absent") }
    return group
}

func benchNewGroup(_ client: BenchClient, _ members: [String]) async throws -> BenchGroup {
    try await client.conversations.newGroup(with: members)
}

func benchPrepare(_ group: BenchGroup, _ row: FixtureMessage, _ ids: [String], _: String) async throws -> String {
    if let parent = row.reply_to {
        return try await group.prepareMessage(content: Reply(reference: ids[Int(parent)!], content: row.text!, contentType: ContentTypeText), noSend: false)
    }
    if let attachment = row.attachment {
        return try await group.prepareMessage(content: Attachment(filename: attachment.filename, mimeType: attachment.mime_type,
                                                                  data: unhex(attachment.bytes_hex)), noSend: false)
    }
    return try await group.prepareMessage(content: row.text!, noSend: false)
}

func benchReact(_ group: BenchGroup, _ id: String, _ inbox: String, _ reaction: FixtureReaction) async throws -> String {
    try await group.prepareMessage(content: Reaction(reference: id, action: .added, content: reaction.content,
                                                     schema: .unicode, referenceInboxId: inbox), options: SendOptions(contentType: ContentTypeReactionV2), noSend: false)
}

func benchPublish(_ group: BenchGroup) async throws {
    try await group.publishMessages()
}

func benchGroupSync(_ group: BenchGroup) async throws {
    try await group.sync()
}

func benchStream(_: BenchClient, _ group: BenchGroup) async throws -> AsyncThrowingStream<BenchLiveMessage, Error> {
    let source = group.streamMessages()
    return AsyncThrowingStream { continuation in
        let task = Task {
            do { for try await value in source {
                continuation.yield(value)
            }; continuation.finish() } catch { continuation.finish(throwing: error) }
        }
        continuation.onTermination = { _ in task.cancel(); group.endStream() }
    }
}

func benchPage(_ group: BenchGroup, _ count: Int, _ keys: [String: String]) async throws -> [Any] {
    let values = try await group.enrichedMessages(limit: count, direction: .ascending,
                                                  excludeContentTypes: [.reaction, .groupUpdated, .groupMembershipChange])
    return try values.map { message in
        guard let key = keys[message.id] else { throw BenchFailure(message: "Unexpected message") }
        var text: String?; var attachment: FixtureAttachment?; var parent: String?; var parentText: String?
        switch message.contentTypeId.typeID {
        case "text": text = try message.content() as String
        case "attachment":
            let value: Attachment = try message.content()
            attachment = FixtureAttachment(filename: value.filename, mime_type: value.mimeType, bytes_hex: hex(value.data))
        case "reply":
            let value: Reply = try message.content(); parent = keys[value.reference]
            text = value.content as? String; parentText = try value.inReplyTo?.content() as String?
            try require(parent != nil && parentText != nil && text != nil, "Reply enrichment is absent")
        default: throw BenchFailure(message: "Unexpected public message type")
        }
        let reactions = try (message.reactions ?? []).map { value -> FixtureReaction in
            let reaction: Reaction = try value.content()
            return FixtureReaction(content: reaction.content, schema: reaction.schema.rawValue, action: reaction.action.rawValue)
        }
        return try jsonRow(FixtureMessage(key: key, text: text, reply_to: parent, parent_text: parentText,
                                          reactions: reactions, attachment: attachment))
    }
}

func benchLift(_: BenchGroup, _: Int) async throws -> [String: Any]? {
    nil
}

func benchLive(_ message: BenchLiveMessage) throws -> LiveEvent {
    let content: Any = try message.content()
    switch content {
    case let value as String: return LiveEvent(id: message.id, kind: "text", text: value)
    case let value as Attachment:
        return LiveEvent(id: message.id, kind: "attachment", attachment: FixtureAttachment(filename: value.filename,
                                                                                           mime_type: value.mimeType, bytes_hex: hex(value.data)))
    case let value as Reply:
        guard let text = value.content as? String else { throw LiveFailure(message: "Missing live reply body") }
        return try LiveEvent(id: message.id, kind: "reply", text: text, reference: value.reference,
                             eager_parent_text: value.inReplyTo?.content() as String?)
    case let value as Reaction:
        return LiveEvent(id: message.id, kind: "reaction", reference: value.reference,
                         reaction: FixtureReaction(content: value.content, schema: value.schema.rawValue, action: value.action.rawValue))
    default: throw LiveFailure(message: "Missing or unsupported live content")
    }
}
