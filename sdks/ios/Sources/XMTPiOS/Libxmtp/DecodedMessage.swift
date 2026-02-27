import Foundation

enum DecodedMessageError: Error {
	case decodeError(String)
}

/// The delivery status of a message on the XMTP network.
public enum MessageDeliveryStatus: String, Sendable {
	/// Matches all delivery statuses (used as a filter value).
	case all
	/// The message has been published to the network.
	case published
	/// The message is stored locally but not yet published.
	case unpublished
	/// The message failed to publish to the network.
	case failed

	func toFfi() -> FfiDeliveryStatus? {
		switch self {
		case .all:
			nil
		case .published:
			.published
		case .unpublished:
			.unpublished
		case .failed:
			.failed
		}
	}

	static func fromFfi(_ ffiStatus: FfiDeliveryStatus) -> MessageDeliveryStatus {
		switch ffiStatus {
		case .published:
			.published
		case .unpublished:
			.unpublished
		case .failed:
			.failed
		}
	}
}

/// The sort order for message queries.
public enum SortDirection {
	/// Oldest messages first.
	case ascending
	/// Newest messages first (default).
	case descending

	func toFfi() -> FfiDirection {
		switch self {
		case .ascending:
			.ascending
		case .descending:
			.descending
		}
	}

	static func fromFfi(_ ffiDirection: FfiDirection) -> SortDirection {
		switch ffiDirection {
		case .ascending:
			.ascending
		case .descending:
			.descending
		}
	}
}

/// The field used to sort messages in query results.
public enum MessageSortBy {
	/// Sort by the timestamp the message was sent on the network.
	case sentAt
	/// Sort by the timestamp the message was inserted into the local database.
	case insertedAt

	func toFfi() -> FfiSortBy {
		switch self {
		case .sentAt:
			.sentAt
		case .insertedAt:
			.insertedAt
		}
	}

	static func fromFfi(_ ffiSortBy: FfiSortBy) -> MessageSortBy {
		switch ffiSortBy {
		case .sentAt:
			.sentAt
		case .insertedAt:
			.insertedAt
		}
	}
}

/// A decoded message from an XMTP conversation.
///
/// `DecodedMessage` wraps a message received from the network with its decoded content.
/// Use ``content()`` to extract the typed payload, or ``body`` for a plain-text representation.
///
/// For messages with enriched metadata (reactions, replies baked in), see ``DecodedMessageV2``.
public struct DecodedMessage: Identifiable {
	let ffiMessage: FfiMessage
	private let decodedContent: Any?

	/// Child messages associated with this message (e.g., reactions when using `messagesWithReactions`).
	public let childMessages: [DecodedMessage]?

	/// The hex-encoded unique identifier of this message.
	public var id: String {
		ffiMessage.id.toHex
	}

	/// The hex-encoded identifier of the conversation this message belongs to.
	public var conversationId: String {
		ffiMessage.conversationId.toHex
	}

	/// The inbox ID of the account that sent this message.
	public var senderInboxId: InboxId {
		ffiMessage.senderInboxId
	}

	/// The kind of conversation message (e.g., application, membership change).
	public var kind: FfiConversationMessageKind {
		ffiMessage.kind
	}

	/// The date when this message was sent on the network.
	public var sentAt: Date {
		Date(
			timeIntervalSince1970: TimeInterval(ffiMessage.sentAtNs)
				/ 1_000_000_000
		)
	}

	/// The timestamp in nanoseconds when this message was sent on the network.
	public var sentAtNs: Int64 {
		ffiMessage.sentAtNs
	}

	/// The date when this message was inserted into the local database.
	public var insertedAt: Date {
		Date(
			timeIntervalSince1970: TimeInterval(ffiMessage.insertedAtNs)
				/ 1_000_000_000
		)
	}

	/// The timestamp in nanoseconds when this message was inserted into the local database.
	public var insertedAtNs: Int64 {
		ffiMessage.insertedAtNs
	}

	/// The timestamp in nanoseconds when this message expires, or `nil` if it does not expire.
	public var expiresAtNs: Int64? {
		ffiMessage.expireAtNs
	}

	/// The date when this message expires, or `nil` if it does not expire.
	public var expiresAt: Date? {
		expiresAtNs.map { Date(timeIntervalSince1970: TimeInterval($0) / 1_000_000_000) }
	}

	/// The current delivery status of this message.
	public var deliveryStatus: MessageDeliveryStatus {
		switch ffiMessage.deliveryStatus {
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

	/// Extracts the decoded content of this message as the specified type.
	///
	/// The content type depends on the codec used to send the message.
	/// For plain text messages, use `String`. For reactions, use `Reaction`, etc.
	///
	/// ```swift
	/// let text: String = try message.content()
	/// ```
	///
	/// - Returns: The decoded content cast to the requested type.
	/// - Throws: ``DecodedMessageError/decodeError(_:)`` if the content cannot be cast to type `T`.
	public func content<T>() throws -> T {
		guard let result = decodedContent as? T else {
			throw DecodedMessageError.decodeError(
				"Decoded content could not be cast to the expected type \(T.self)."
			)
		}
		return result
	}

	/// The fallback text representation of this message's content.
	public var fallback: String {
		get throws {
			try encodedContent.fallback
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

	/// The raw encoded content of this message before codec decoding.
	public var encodedContent: EncodedContent {
		get throws {
			try EncodedContent(serializedBytes: ffiMessage.content)
		}
	}

	/// Creates a `DecodedMessage` from an FFI message, or returns `nil` if decoding fails.
	public static func create(ffiMessage: FfiMessage)
		-> DecodedMessage?
	{
		do {
			let encodedContent = try EncodedContent(
				serializedBytes: ffiMessage.content
			)
			if encodedContent.type == ContentTypeGroupUpdated,
			   ffiMessage.kind != .membershipChange
			{
				throw DecodedMessageError.decodeError(
					"Error decoding group membership change"
				)
			}
			// Decode the content once during creation
			let decodedContent: Any = try encodedContent.decoded()
			return DecodedMessage(
				ffiMessage: ffiMessage, decodedContent: decodedContent,
				childMessages: nil
			)
		} catch {
			print("Error creating Message: \(error)")
			return nil
		}
	}

	/// Creates a `DecodedMessage` from an FFI message with reactions, or returns `nil` if decoding fails.
	public static func create(ffiMessage: FfiMessageWithReactions)
		-> DecodedMessage?
	{
		do {
			let encodedContent = try EncodedContent(
				serializedBytes: ffiMessage.message.content
			)
			if encodedContent.type == ContentTypeGroupUpdated,
			   ffiMessage.message.kind != .membershipChange
			{
				throw DecodedMessageError.decodeError(
					"Error decoding group membership change"
				)
			}
			// Decode the content once during creation
			let decodedContent: Any = try encodedContent.decoded()

			let childMessages = try ffiMessage.reactions.map { reaction in
				let encodedContent = try EncodedContent(
					serializedBytes: reaction.content
				)
				// Decode the content once during creation
				let decodedContent: Any = try encodedContent.decoded()
				return DecodedMessage(
					ffiMessage: reaction, decodedContent: decodedContent,
					childMessages: nil
				)
			}

			return DecodedMessage(
				ffiMessage: ffiMessage.message, decodedContent: decodedContent,
				childMessages: childMessages
			)
		} catch {
			print("Error creating Message: \(error)")
			return nil
		}
	}
}
