import Foundation

/// A type representing a structured intent from a message.
public typealias Intent = FfiIntent
/// A type representing structured actions from a message.
public typealias Actions = FfiActions

/// An enriched decoded message with reactions and replies baked in.
///
/// Unlike ``DecodedMessage``, `DecodedMessageV2` includes enriched metadata such as
/// reactions and the original message for replies directly on the message object.
/// This eliminates the need for separate queries and is recommended for UI rendering.
///
/// Retrieve enriched messages via ``Group/enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
/// or ``Dm/enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``.
///
/// ```swift
/// let messages = try await group.enrichedMessages(limit: 20)
/// for message in messages {
///     let text: String = try message.content()
///     if let reactions = message.reactions {
///         // Handle reactions
///     }
/// }
/// ```
public struct DecodedMessageV2: Identifiable {
	private let ffiMessage: FfiDecodedMessage

	/// The hex-encoded unique identifier of this message.
	public var id: String {
		ffiMessage.id().toHex
	}

	/// The hex-encoded identifier of the conversation this message belongs to.
	public var conversationId: String {
		ffiMessage.conversationId().toHex
	}

	/// The inbox ID of the account that sent this message.
	public var senderInboxId: InboxId {
		ffiMessage.senderInboxId()
	}

	/// The date when this message was sent on the network.
	public var sentAt: Date {
		Date(timeIntervalSince1970: TimeInterval(ffiMessage.sentAtNs()) / 1_000_000_000)
	}

	/// The timestamp in nanoseconds when this message was sent on the network.
	public var sentAtNs: Int64 {
		ffiMessage.sentAtNs()
	}

	/// The date when this message was inserted into the local database.
	public var insertedAt: Date {
		Date(timeIntervalSince1970: TimeInterval(ffiMessage.insertedAtNs()) / 1_000_000_000)
	}

	/// The timestamp in nanoseconds when this message was inserted into the local database.
	public var insertedAtNs: Int64 {
		ffiMessage.insertedAtNs()
	}

	/// The timestamp in nanoseconds when this message expires, or `nil` if it does not expire.
	public var expiresAtNs: Int64? {
		ffiMessage.expiresAtNs()
	}

	/// The date when this message expires, or `nil` if it does not expire.
	public var expiresAt: Date? {
		expiresAtNs.map { Date(timeIntervalSince1970: TimeInterval($0) / 1_000_000_000) }
	}

	/// The current delivery status of this message.
	public var deliveryStatus: MessageDeliveryStatus {
		switch ffiMessage.deliveryStatus() {
		case .unpublished:
			.unpublished
		case .published:
			.published
		case .failed:
			.failed
		}
	}

	/// The MLS topic string for the conversation this message belongs to.
	public var topic: String {
		Topic.groupMessage(conversationId).description
	}

	/// The reactions associated with this message, or `nil` if there are none.
	///
	/// Each reaction is itself a `DecodedMessageV2` whose content can be decoded
	/// as a `Reaction` type.
	public var reactions: [DecodedMessageV2]? {
		let reactionMessages = ffiMessage.reactions()
		guard !reactionMessages.isEmpty else { return nil }
		return reactionMessages.compactMap { DecodedMessageV2(ffiMessage: $0) }
	}

	/// Extracts the decoded content of this message as the specified type.
	///
	/// The content is decoded on each call from the enriched FFI representation.
	/// For plain text messages, use `String`. For reactions, use `Reaction`, etc.
	///
	/// ```swift
	/// let text: String = try message.content()
	/// ```
	///
	/// - Returns: The decoded content cast to the requested type.
	/// - Throws: ``DecodedMessageError/decodeError(_:)`` if the content cannot be cast to type `T`.
	public func content<T>() throws -> T {
		let decodedContent = try mapContent(ffiMessage.content())
		guard let result = decodedContent as? T else {
			throw DecodedMessageError.decodeError(
				"Decoded content could not be cast to the expected type \(T.self)."
			)
		}
		return result
	}

	/// The fallback text representation of this message's content.
	///
	/// Returns the codec-provided fallback text if available, otherwise
	/// attempts to extract a human-readable string from the content.
	public var fallback: String {
		get throws {
			if let fallbackText = ffiMessage.fallbackText(), !fallbackText.isEmpty {
				return fallbackText
			}
			switch ffiMessage.content() {
			case let .text(text):
				return text.content
			case let .custom(encodedContent):
				return encodedContent.fallback ?? ""
			case .leaveRequest:
				return "A member has requested leaving the group"
			default:
				return ""
			}
		}
	}

	/// A plain-text representation of this message.
	///
	/// Returns the decoded `String` content if available, otherwise falls back
	/// to the fallback text provided by the codec.
	public var body: String {
		get throws {
			do {
				return try content() as String
			} catch {
				return try fallback
			}
		}
	}

	/// The content type identifier for this message (e.g., text, reaction, attachment).
	public var contentTypeId: ContentTypeID {
		let ffiContentType = ffiMessage.contentTypeId()
		return ContentTypeID(
			authorityID: ffiContentType.authorityId,
			typeID: ffiContentType.typeId,
			versionMajor: Int(ffiContentType.versionMajor),
			versionMinor: Int(ffiContentType.versionMinor)
		)
	}

	/// Creates a `DecodedMessageV2` from an FFI decoded message, or returns `nil` if the message is invalid.
	public init?(ffiMessage: FfiDecodedMessage) {
		self.ffiMessage = ffiMessage
	}

	/// Creates a `DecodedMessageV2` from an FFI decoded message, or returns `nil` if the message is invalid.
	public static func create(ffiMessage: FfiDecodedMessage) -> DecodedMessageV2? {
		DecodedMessageV2(ffiMessage: ffiMessage)
	}

	private func mapContent(_ content: FfiDecodedMessageContent) throws -> Any {
		switch content {
		case let .text(textContent):
			return textContent.content

		case let .markdown(markdownContent):
			return markdownContent.content

		case let .reply(enrichedReply):
			return try mapReply(enrichedReply)

		case let .reaction(reactionPayload):
			return mapReaction(reactionPayload)

		case let .attachment(ffiAttachment):
			return mapAttachment(ffiAttachment)

		case let .remoteAttachment(ffiAttachment):
			return try mapRemoteAttachment(ffiAttachment)

		case let .multiRemoteAttachment(multiAttachment):
			return mapMultiRemoteAttachment(multiAttachment)

		case let .transactionReference(txRef):
			return mapTransactionReference(txRef)

		case let .groupUpdated(groupUpdated):
			return mapGroupUpdated(groupUpdated)

		case .readReceipt:
			return ReadReceipt()

		case let .leaveRequest(ffiLeaveRequest):
			return mapLeaveRequest(ffiLeaveRequest)

		case let .walletSendCalls(walletSend):
			return walletSend

		case let .custom(ffiEncodedContent):
			let encoded = try mapFfiEncodedContent(ffiEncodedContent)
			let codec = Client.codecRegistry.find(for: encoded.type)
			return try codec.decode(content: encoded)

		case let .intent(intent):
			return intent

		case let .actions(actions):
			return actions

		case let .deletedMessage(ffiDeletedMessage):
			return mapDeletedMessage(ffiDeletedMessage)
		}
	}

	private func mapDeletedMessage(_ ffiDeletedMessage: FfiDeletedMessage) -> DeletedMessage {
		let deletedBy: DeletedBy = switch ffiDeletedMessage.deletedBy {
		case .sender:
			.sender
		case let .admin(inboxId):
			.admin(inboxId: inboxId)
		}
		return DeletedMessage(deletedBy: deletedBy)
	}

	private func mapLeaveRequest(_ ffiLeaveRequest: FfiLeaveRequest) -> LeaveRequest {
		LeaveRequest(authenticatedNote: ffiLeaveRequest.authenticatedNote)
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
		case let .text(textContent):
			return textContent.content
		case let .markdown(markdownContent):
			return markdownContent.content
		case let .reaction(reactionPayload):
			return mapReaction(reactionPayload)
		case let .attachment(ffiAttachment):
			return mapAttachment(ffiAttachment)
		case let .remoteAttachment(ffiAttachment):
			return try mapRemoteAttachment(ffiAttachment)
		case let .multiRemoteAttachment(multiAttachment):
			return mapMultiRemoteAttachment(multiAttachment)
		case let .transactionReference(txRef):
			return mapTransactionReference(txRef)
		case let .groupUpdated(groupUpdated):
			return mapGroupUpdated(groupUpdated)
		case let .walletSendCalls(walletSend):
			return walletSend
		case .readReceipt:
			return ReadReceipt()
		case let .leaveRequest(ffiLeaveRequest):
			return mapLeaveRequest(ffiLeaveRequest)
		case let .custom(ffiEncodedContent):
			let encoded = try mapFfiEncodedContent(ffiEncodedContent)
			let codec = Client.codecRegistry.find(for: encoded.type)
			return try codec.decode(content: encoded)
		case let .intent(intent):
			return intent as Intent
		case let .actions(actions):
			return actions as Actions
		case let .deletedMessage(ffiDeletedMessage):
			return mapDeletedMessage(ffiDeletedMessage)
		}
	}

	private func determineContentType(from body: FfiDecodedMessageBody) -> ContentTypeID {
		switch body {
		case .text:
			ContentTypeText
		case .markdown:
			ContentTypeID(
				authorityID: "xmtp.org",
				typeID: "markdown",
				versionMajor: 1,
				versionMinor: 0
			)
		case .reaction:
			ContentTypeReaction
		case .attachment:
			ContentTypeAttachment
		case .remoteAttachment:
			ContentTypeRemoteAttachment
		case .multiRemoteAttachment:
			ContentTypeMultiRemoteAttachment
		case .transactionReference:
			ContentTypeTransactionReference
		case .groupUpdated:
			ContentTypeGroupUpdated
		case .readReceipt:
			ContentTypeReadReceipt
		case .leaveRequest:
			ContentTypeLeaveRequest
		case .walletSendCalls:
			ContentTypeID(
				authorityID: "xmtp.org",
				typeID: "walletSendCalls",
				versionMajor: 1,
				versionMinor: 0
			)
		case let .custom(ffiEncodedContent):
			if let typeId = ffiEncodedContent.typeId {
				ContentTypeID(
					authorityID: typeId.authorityId,
					typeID: typeId.typeId,
					versionMajor: Int(typeId.versionMajor),
					versionMinor: Int(typeId.versionMinor)
				)
			} else {
				// Return a default content type if none is specified
				ContentTypeText
			}
		case .intent:
			ContentTypeID(
				authorityID: "coinbase.com",
				typeID: "intent",
				versionMajor: 1,
				versionMinor: 0
			)
		case .actions:
			ContentTypeID(
				authorityID: "coinbase.com",
				typeID: "actions",
				versionMajor: 1,
				versionMinor: 0
			)
		case .deletedMessage:
			ContentTypeDeletedMessage
		}
	}

	private func mapReaction(_ reactionPayload: FfiReactionPayload) -> Reaction {
		Reaction(
			reference: reactionPayload.reference,
			action: reactionPayload.action == .added ? .added : .removed,
			content: reactionPayload.content,
			schema: mapReactionSchema(reactionPayload.schema),
			referenceInboxId: reactionPayload.referenceInboxId
		)
	}

	private func mapReactionSchema(_ schema: FfiReactionSchema) -> ReactionSchema {
		switch schema {
		case .unicode:
			.unicode
		case .shortcode:
			.shortcode
		case .custom:
			.custom
		case .unknown:
			.unknown
		}
	}

	private func mapRemoteAttachmentScheme(_ scheme: String) -> RemoteAttachment.Scheme {
		RemoteAttachment.Scheme(rawValue: scheme) ?? .https
	}

	private func mapAttachment(_ ffiAttachment: FfiAttachment) -> Attachment {
		Attachment(
			filename: ffiAttachment.filename ?? "",
			mimeType: ffiAttachment.mimeType,
			data: ffiAttachment.content
		)
	}

	private func mapRemoteAttachment(_ ffiAttachment: FfiRemoteAttachment) throws -> RemoteAttachment {
		try RemoteAttachment(
			url: ffiAttachment.url,
			contentDigest: ffiAttachment.contentDigest,
			secret: ffiAttachment.secret,
			salt: ffiAttachment.salt,
			nonce: ffiAttachment.nonce,
			scheme: mapRemoteAttachmentScheme(ffiAttachment.scheme),
			contentLength: ffiAttachment.contentLength.map { Int($0) },
			filename: ffiAttachment.filename
		)
	}

	private func mapMultiRemoteAttachment(_ multiAttachment: FfiMultiRemoteAttachment) -> MultiRemoteAttachment {
		MultiRemoteAttachment(
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
		TransactionReference(
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
		updated.leftInboxes = groupUpdated.leftInboxes.map { inbox in
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
