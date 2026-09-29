import Foundation

enum DecodedMessageError: Error {
	case decodeError(String)
}

public enum MessageDeliveryStatus: String, Sendable {
	case all
	case published
	case unpublished
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

public enum SortDirection {
	case ascending
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

public enum MessageSortBy {
	case sentAt
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

public struct DecodedMessage: Identifiable {
	let ffiMessage: FfiMessage
	private let decodedContent: Any?
	/// Cursor for this stream handoff. Reading it does not acknowledge delivery.
	public let deliveryCursor: FfiDeliveryCursor?
	/// Set when the content could not be decoded: the exact received bytes, the
	/// received identifier and fallback when present, and the typed cause.
	/// `content()` throws for such a message; `fallback` returns the received one.
	public let undecodable: UndecodableContent?

	public var id: String {
		ffiMessage.id.toHex
	}

	public var conversationId: String {
		ffiMessage.conversationId.toHex
	}

	public var senderInboxId: InboxId {
		ffiMessage.senderInboxId
	}

	public var kind: FfiConversationMessageKind {
		ffiMessage.kind
	}

	public var sentAt: Date {
		Date(
			timeIntervalSince1970: TimeInterval(ffiMessage.sentAtNs)
				/ 1_000_000_000
		)
	}

	public var sentAtNs: Int64 {
		ffiMessage.sentAtNs
	}

	public var insertedAt: Date {
		Date(
			timeIntervalSince1970: TimeInterval(ffiMessage.insertedAtNs)
				/ 1_000_000_000
		)
	}

	public var insertedAtNs: Int64 {
		ffiMessage.insertedAtNs
	}

	public var expiresAtNs: Int64? {
		ffiMessage.expireAtNs
	}

	public var expiresAt: Date? {
		expiresAtNs.map { Date(timeIntervalSince1970: TimeInterval($0) / 1_000_000_000) }
	}

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

	public var topic: String {
		Topic.groupMessage(conversationId).description
	}

	public func content<T>() throws -> T {
		guard let result = decodedContent as? T else {
			throw DecodedMessageError.decodeError(
				"Decoded content could not be cast to the expected type \(T.self)."
			)
		}
		return result
	}

	public var fallback: String {
		get throws {
			if let undecodable {
				return undecodable.fallback ?? ""
			}
			return try encodedContent.fallback
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

	public var encodedContent: EncodedContent {
		get throws {
			try EncodedContent(serializedBytes: ffiMessage.content)
		}
	}

	public static func create(ffiMessage: FfiMessage, deliveryCursor: FfiDeliveryCursor? = nil)
		-> DecodedMessage?
	{
		decodeForDelivery(ffiMessage: ffiMessage, deliveryCursor: deliveryCursor)
	}

	/// Return nil only for content that forges a reserved membership change,
	/// which the delivery stream consumes without a handoff. Content that does
	/// not parse or decode is kept as an undecodable message with its exact
	/// bytes, so history keeps the row and the delivery stream hands it off.
	static func decodeForDelivery(ffiMessage: FfiMessage, deliveryCursor: FfiDeliveryCursor? = nil)
		-> DecodedMessage?
	{
		let undecodable = { (encodedContent: EncodedContent?, kind: ContentDecodeFailureKind, message: String) in
			DecodedMessage(
				ffiMessage: ffiMessage,
				decodedContent: nil,
				deliveryCursor: deliveryCursor,
				undecodable: UndecodableContent(
					rawBytes: ffiMessage.content,
					contentType: encodedContent.flatMap { content in
						content.hasType
							? FfiContentTypeId(
								authorityId: content.type.authorityID,
								typeId: content.type.typeID,
								versionMajor: content.type.versionMajor,
								versionMinor: content.type.versionMinor
							)
							: nil
					},
					fallback: encodedContent.map { $0.hasFallback ? $0.fallback : nil } ?? nil,
					failureKind: kind,
					failureMessage: message
				)
			)
		}
		let encodedContent: EncodedContent
		do {
			encodedContent = try EncodedContent(serializedBytes: ffiMessage.content)
		} catch {
			return undecodable(nil, .malformedEnvelope, "\(error)")
		}
		if encodedContent.type == ContentTypeGroupUpdated,
		   ffiMessage.kind != .membershipChange
		{
			return nil
		}
		guard encodedContent.hasType,
		      !encodedContent.type.authorityID.isEmpty,
		      !encodedContent.type.typeID.isEmpty
		else {
			return undecodable(encodedContent, .malformedEnvelope, "content type identifier is absent or incomplete")
		}
		do {
			let decodedContent: Any = try encodedContent.decoded()
			return DecodedMessage(
				ffiMessage: ffiMessage, decodedContent: decodedContent,
				deliveryCursor: deliveryCursor, undecodable: nil
			)
		} catch {
			return undecodable(encodedContent, .codecDecodeFailed, "\(error)")
		}
	}
}
