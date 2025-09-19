import Foundation
import LibXMTP

public struct DecodedMessageV2: Identifiable {
    private let ffiMessage: FfiDecodedMessage

    public var id: String {
        ffiMessage.id().toHex
    }

    public var conversationId: String {
        ffiMessage.conversationId().toHex
    }

    public var senderInboxId: InboxId {
        ffiMessage.senderInboxId()
    }

    public var sentAt: Date {
        Date(timeIntervalSince1970: TimeInterval(ffiMessage.sentAtNs()) / 1_000_000_000)
    }

    public var sentAtNs: Int64 {
        ffiMessage.sentAtNs()
    }

    public var deliveryStatus: MessageDeliveryStatus {
        switch ffiMessage.deliveryStatus() {
        case .unpublished:
            return .unpublished
        case .published:
            return .published
        case .failed:
            return .failed
        }
    }

    public var topic: String {
        Topic.groupMessage(conversationId).description
    }

    public var reactions: [DecodedMessageV2]? {
        let reactionMessages = ffiMessage.reactions()
        guard !reactionMessages.isEmpty else { return nil }
        return reactionMessages.compactMap { DecodedMessageV2(ffiMessage: $0) }
    }

    public func content<T>() throws -> T {
        let decodedContent = try mapContent(ffiMessage.content())
        guard let result = decodedContent as? T else {
            throw DecodedMessageError.decodeError(
                "Decoded content could not be cast to the expected type \(T.self)."
            )
        }
        return result
    }

    public var fallback: String {
        get throws {
            if let fallbackText = ffiMessage.fallbackText(), !fallbackText.isEmpty {
                return fallbackText
            }
            switch ffiMessage.content() {
            case .text(let text):
                return text.content
            case .custom(let encodedContent):
                return encodedContent.fallback ?? ""
            default:
                return ""
            }
        }
    }

    public var body: String {
        get throws {
            do {
                return try content() as String
            } catch {
                return try fallback
            }
        }
    }

    public var contentTypeId: ContentTypeID {
        let ffiContentType = ffiMessage.contentTypeId()
        return ContentTypeID(
            authorityID: ffiContentType.authorityId,
            typeID: ffiContentType.typeId,
            versionMajor: Int(ffiContentType.versionMajor),
            versionMinor: Int(ffiContentType.versionMinor)
        )
    }

    public init?(ffiMessage: FfiDecodedMessage) {
        self.ffiMessage = ffiMessage
    }

    public static func create(ffiMessage: FfiDecodedMessage) -> DecodedMessageV2? {
        return DecodedMessageV2(ffiMessage: ffiMessage)
    }

    private func mapContent(_ content: FfiDecodedMessageContent) throws -> Any {
        switch content {
        case .text(let textContent):
            return textContent.content

        case .reply(let enrichedReply):
            return try mapReply(enrichedReply)

        case .reaction(let reactionPayload):
            return mapReaction(reactionPayload)

        case .attachment(let ffiAttachment):
            return mapAttachment(ffiAttachment)

        case .remoteAttachment(let ffiAttachment):
            return try mapRemoteAttachment(ffiAttachment)

        case .multiRemoteAttachment(let multiAttachment):
            return mapMultiRemoteAttachment(multiAttachment)

        case .transactionReference(let txRef):
            return mapTransactionReference(txRef)

        case .groupUpdated(let groupUpdated):
            return mapGroupUpdated(groupUpdated)

        case .readReceipt(_):
            return ReadReceipt()

        case .walletSendCalls(let walletSend):
            return walletSend

        case .custom(let ffiEncodedContent):
            let encoded = try mapFfiEncodedContent(ffiEncodedContent)
            let codec = Client.codecRegistry.find(for: encoded.type)
            return try codec.decode(content: encoded)
        }
    }

    private func mapReply(_ enrichedReply: FfiEnrichedReply) throws -> Reply {
        guard let bodyContent = enrichedReply.content else {
            throw DecodedMessageError.decodeError("Missing reply content")
        }

        let content = try mapMessageBody(bodyContent)
        let contentType = determineContentType(from: bodyContent)

        var reply = Reply(
            reference: enrichedReply.inReplyTo?.id().toHex ?? "",
            content: content,
            contentType: contentType
        )

        if let inReplyToMessage = enrichedReply.inReplyTo {
            reply.inReplyTo = DecodedMessageV2(ffiMessage: inReplyToMessage)
        }

        return reply
    }

    private func mapMessageBody(_ body: FfiDecodedMessageBody) throws -> Any {
        switch body {
        case .text(let textContent):
            return textContent.content
        case .reaction(let reactionPayload):
            return mapReaction(reactionPayload)
        case .attachment(let ffiAttachment):
            return mapAttachment(ffiAttachment)
        case .remoteAttachment(let ffiAttachment):
            return try mapRemoteAttachment(ffiAttachment)
        case .multiRemoteAttachment(let multiAttachment):
            return mapMultiRemoteAttachment(multiAttachment)
        case .transactionReference(let txRef):
            return mapTransactionReference(txRef)
        case .groupUpdated(let groupUpdated):
            return mapGroupUpdated(groupUpdated)
        case .walletSendCalls(let walletSend):
            return walletSend
        case .readReceipt(_):
            return ReadReceipt()
        case .custom(let ffiEncodedContent):
            let encoded = try mapFfiEncodedContent(ffiEncodedContent)
            let codec = Client.codecRegistry.find(for: encoded.type)
            return try codec.decode(content: encoded)
        }
    }

    private func determineContentType(from body: FfiDecodedMessageBody) -> ContentTypeID {
        switch body {
        case .text:
            return ContentTypeText
        case .reaction:
            return ContentTypeReaction
        case .attachment:
            return ContentTypeAttachment
        case .remoteAttachment:
            return ContentTypeRemoteAttachment
        case .multiRemoteAttachment:
            return ContentTypeMultiRemoteAttachment
        case .transactionReference:
            return ContentTypeTransactionReference
        case .groupUpdated:
            return ContentTypeGroupUpdated
        case .readReceipt:
            return ContentTypeReadReceipt
        case .walletSendCalls:
            return ContentTypeID(
                authorityID: "xmtp.org",
                typeID: "walletSendCalls",
                versionMajor: 1,
                versionMinor: 0
            )
        case .custom(let ffiEncodedContent):
            if let typeId = ffiEncodedContent.typeId {
                return ContentTypeID(
                    authorityID: typeId.authorityId,
                    typeID: typeId.typeId,
                    versionMajor: Int(typeId.versionMajor),
                    versionMinor: Int(typeId.versionMinor)
                )
            } else {
                // Return a default content type if none is specified
                return ContentTypeText
            }
        }
    }

    private func mapReaction(_ reactionPayload: FfiReactionPayload) -> Reaction {
        return Reaction(
            reference: reactionPayload.reference,
            action: reactionPayload.action == .added ? .added : .removed,
            content: reactionPayload.content,
            schema: mapReactionSchema(reactionPayload.schema)
        )
    }

    private func mapReactionSchema(_ schema: FfiReactionSchema) -> ReactionSchema {
        switch schema {
        case .unicode:
            return .unicode
        case .shortcode:
            return .shortcode
        case .custom:
            return .custom
        case .unknown:
            return .unknown
        }
    }

    private func mapRemoteAttachmentScheme(_ scheme: String) -> RemoteAttachment.Scheme {
        return RemoteAttachment.Scheme(rawValue: scheme) ?? .https
    }

    private func mapAttachment(_ ffiAttachment: FfiAttachment) -> Attachment {
        return Attachment(
            filename: ffiAttachment.filename ?? "",
            mimeType: ffiAttachment.mimeType,
            data: ffiAttachment.content
        )
    }

    private func mapRemoteAttachment(_ ffiAttachment: FfiRemoteAttachment) throws -> RemoteAttachment {
        return try RemoteAttachment(
            url: ffiAttachment.url,
            contentDigest: ffiAttachment.contentDigest,
            secret: ffiAttachment.secret,
            salt: ffiAttachment.salt,
            nonce: ffiAttachment.nonce,
            scheme: mapRemoteAttachmentScheme(ffiAttachment.scheme),
            contentLength: Int(ffiAttachment.contentLength),
            filename: ffiAttachment.filename
        )
    }

    private func mapMultiRemoteAttachment(_ multiAttachment: FfiMultiRemoteAttachment) -> MultiRemoteAttachment {
        return MultiRemoteAttachment(
            remoteAttachments: multiAttachment.attachments.map { info in
                MultiRemoteAttachment.RemoteAttachmentInfo(
                    url: info.url,
                    filename: info.filename ?? "",
                    contentLength: info.contentLength ?? 0,
                    contentDigest: info.contentDigest,
                    nonce: info.nonce,
                    scheme: info.scheme,
                    salt: info.salt,
                    secret: info.secret
                )
            }
        )
    }

    private func mapTransactionReference(_ txRef: FfiTransactionReference) -> TransactionReference {
        return TransactionReference(
            namespace: txRef.namespace,
            networkId: txRef.networkId,
            reference: txRef.reference,
            metadata: txRef.metadata.map { metadata in
                TransactionReference.Metadata(
                    transactionType: metadata.transactionType,
                    currency: metadata.currency,
                    amount: metadata.amount,
                    decimals: metadata.decimals,
                    fromAddress: metadata.fromAddress,
                    toAddress: metadata.toAddress
                )
            }
        )
    }

    private func mapGroupUpdated(_ groupUpdated: FfiGroupUpdated) -> GroupUpdated {
        var updated = GroupUpdated()
        updated.initiatedByInboxID = groupUpdated.initiatedByInboxId
        updated.addedInboxes = groupUpdated.addedInboxes.map { inbox in
            var inboxEntry = GroupUpdated.Inbox()
            inboxEntry.inboxID = inbox.inboxId
            return inboxEntry
        }
        updated.removedInboxes = groupUpdated.removedInboxes.map { inbox in
            var inboxEntry = GroupUpdated.Inbox()
            inboxEntry.inboxID = inbox.inboxId
            return inboxEntry
        }
        updated.metadataFieldChanges = groupUpdated.metadataFieldChanges.map { change in
            var fieldChange = GroupUpdated.MetadataFieldChange()
            fieldChange.fieldName = change.fieldName
            fieldChange.oldValue = change.oldValue ?? ""
            fieldChange.newValue = change.newValue ?? ""
            return fieldChange
        }
        return updated
    }

    private func mapFfiEncodedContent(_ ffiContent: FfiEncodedContent) throws -> EncodedContent {
        var encoded = EncodedContent()
        if let typeId = ffiContent.typeId {
            encoded.type = ContentTypeID(
                authorityID: typeId.authorityId,
                typeID: typeId.typeId,
                versionMajor: Int(typeId.versionMajor),
                versionMinor: Int(typeId.versionMinor)
            )
        }
        encoded.parameters = ffiContent.parameters
        encoded.fallback = ffiContent.fallback ?? ""
        encoded.content = ffiContent.content
        if let compression = ffiContent.compression {
            // Map Int32 compression values to the enum
            // Assuming 0 = deflate, 1 = gzip based on the proto definition
            switch compression {
            case 0:
                encoded.compression = .deflate
            case 1:
                encoded.compression = .gzip
            default:
                // Unknown compression value, leave it unset
                break
            }
        }
        return encoded
    }
}
