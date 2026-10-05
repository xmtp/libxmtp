import Foundation
import XmtpSdk

typealias BenchClient = SDKClient
typealias BenchGroup = Group
/// Signs through the host's HTTP signer, which holds a generated test key.
final class BenchSigner: Signer, @unchecked Sendable {
    let config: HostConfig; let key: String; let address: String
    init(_ config: HostConfig, _ key: String, _ address: String) {
        self.config = config; self.key = key; self.address = address
    }

    func identity() async throws -> PublicIdentity {
        PublicIdentity(identifier: address, kind: .ethereum)
    }

    func kind() async throws -> SignerKind {
        .eoa
    }

    func sign(request: SigningRequest) async throws -> Signature {
        try await .ecdsa(unhex(signerHelper(config, ["key": key, "text": request.text])["signature"]!))
    }
}

func benchOptions(_ config: HostConfig, _ path: String) throws -> ClientOptions {
    try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
    return ClientOptions(backend: .options(options: BackendOptions(url: config.backend_url)),
                         storage: StorageOptions(location: .explicit(dbPath: path + "/client.db", attachmentsDir: path + "/attachments"),
                                                 encryptionKey: Data(repeating: 7, count: 32)), deviceSync: false)
}

func benchCreate(_ config: HostConfig, _ key: String, _ address: String, _ path: String) async throws -> BenchClient {
    try await SDKClient.create(signer: BenchSigner(config, key, address), options: benchOptions(config, path))
}

func benchOpen(_ config: HostConfig, _ address: String, _ path: String, _ inbox: String) async throws -> BenchClient {
    try await SDKClient.build(identity: PublicIdentity(identifier: address, kind: .ethereum), options: benchOptions(config, path), inboxId: inbox)
}

func benchClose(_ client: BenchClient) async throws {
    try await client.end()
}

func benchInbox(_ client: BenchClient) -> String {
    client.inboxId()
}

func benchSync(_ client: BenchClient) async throws {
    try await client.conversations().sync()
}

func benchGroupID(_ group: BenchGroup) -> String {
    group.id()
}

func benchGroup(_ client: BenchClient, _ id: String) async throws -> BenchGroup {
    guard case let .group(group)? = try await client.conversations().getById(id: id) else { throw BenchFailure(message: "Seeded group is absent") }
    return group
}

func benchNewGroup(_ client: BenchClient, _ members: [String]) async throws -> BenchGroup {
    try await client.conversations().createGroup(members: members, options: nil)
}

func benchPrepare(_ group: BenchGroup, _ row: FixtureMessage, _ ids: [String], _ inbox: String) async throws -> String {
    let content: StandardContent
    if let parent = row.reply_to {
        content = try .reply(reference: ids[Int(parent)!], referenceInboxId: inbox, content: TextCodec().encode(row.text!))
    } else if let attachment = row.attachment {
        content = .attachment(Attachment(filename: attachment.filename, mimeType: attachment.mime_type, content: unhex(attachment.bytes_hex)))
    } else {
        content = .text(row.text!)
    }
    return try await group.prepareMessage(encoded: encodeStandard(value: content))
}

func benchReact(_ group: BenchGroup, _ id: String, _ inbox: String, _ reaction: FixtureReaction) async throws -> String {
    try await group.prepareMessage(encoded: encodeStandard(value: .reaction(reference: id, referenceInboxId: inbox,
                                                                            reaction: Reaction(content: reaction.content, action: .added, schema: .unicode))))
}

func benchPublish(_ group: BenchGroup) async throws {
    try await group.publishMessages()
}

func benchGroupSync(_ group: BenchGroup) async throws {
    try await group.sync()
}

func benchStream(_ client: BenchClient, _ group: BenchGroup) async throws -> AsyncThrowingStream<Message, Error> {
    let source = try await client.messages(in: group)
    return AsyncThrowingStream { continuation in
        let task = Task {
            do { for try await value in source {
                continuation.yield(value)
            }; continuation.finish() } catch { continuation.finish(throwing: error) }
        }
        continuation.onTermination = { _ in task.cancel() }
    }
}

func benchRows(_ group: BenchGroup, _ count: Int) async throws -> [Message] {
    try await group.messages(options: ListMessagesOptions(limit: UInt32(count), direction: .ascending,
                                                          contentTypes: ["text", "reply", "attachment"].map { ContentTypeId(authorityId: "xmtp.org", typeId: $0, versionMajor: 1, versionMinor: 0) }))
}

func benchPage(_ group: BenchGroup, _ count: Int, _ keys: [String: String]) async throws -> [Any] {
    try await benchRows(group, count).map { message in
        guard let key = keys[message.id], case let .standard(content) = message.content else { throw BenchFailure(message: "Unexpected public content") }
        var text: String?; var attachment: FixtureAttachment?; var parent: String?; var parentText: String?
        switch content {
        case let .text(value): text = value
        case let .attachment(value): attachment = FixtureAttachment(filename: value.filename!, mime_type: value.mimeType, bytes_hex: hex(value.content))
        case let .reply(reference, body):
            parent = keys[reference]
            guard case let .text(value) = body, case let .standard(.text(original))? = message.inReplyToContent,
                  parent != nil else { throw BenchFailure(message: "Reply enrichment is absent") }
            text = value; parentText = original
        default: throw BenchFailure(message: "Unexpected public message type")
        }
        let reactions = message.reactions.map { FixtureReaction(content: $0.reaction.content,
                                                                schema: $0.reaction.schema == .unicode ? "unicode" : "unexpected",
                                                                action: $0.reaction.action == .added ? "added" : "unexpected") }
        return try jsonRow(FixtureMessage(key: key, text: text, reply_to: parent, parent_text: parentText,
                                          reactions: reactions, attachment: attachment))
    }
}
