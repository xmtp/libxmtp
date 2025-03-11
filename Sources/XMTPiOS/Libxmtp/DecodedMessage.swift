import Foundation
import LibXMTP

enum DecodedMessageError: Error {
	case decodeError(String)
}

public enum MessageDeliveryStatus: String, Sendable {
	case all
	case published
	case unpublished
	case failed
}

public enum SortDirection {
	case ascending
	case descending
}

public struct DecodedMessage: Identifiable {
	let ffiMessage: FfiMessage
	private let decodedContent: Any?
	public let childMessages: [DecodedMessage]?

	public var id: String {
		ffiMessage.id.toHex
	}

	public var conversationId: String {
		ffiMessage.conversationId.toHex
	}

	public var senderInboxId: InboxId {
		ffiMessage.senderInboxId
	}

	public var sentAt: Date {
		Date(
			timeIntervalSince1970: TimeInterval(ffiMessage.sentAtNs)
				/ 1_000_000_000)
	}

	public var sentAtNs: Int64 {
		ffiMessage.sentAtNs
	}

	public var deliveryStatus: MessageDeliveryStatus {
		switch ffiMessage.deliveryStatus {
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
			try encodedContent.fallback
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

	public static func create(ffiMessage: FfiMessage)
		-> DecodedMessage?
	{
		do {
			let encodedContent = try EncodedContent(
				serializedBytes: ffiMessage.content)
			if encodedContent.type == ContentTypeGroupUpdated
				&& ffiMessage.kind != .membershipChange
			{
				throw DecodedMessageError.decodeError(
					"Error decoding group membership change")
			}
			// Decode the content once during creation
			let decodedContent: Any = try encodedContent.decoded()
			return DecodedMessage(
				ffiMessage: ffiMessage, decodedContent: decodedContent,
				childMessages: nil)
		} catch {
			print("Error creating Message: \(error)")
			return nil
		}
	}

	public static func create(ffiMessage: FfiMessageWithReactions)
		-> DecodedMessage?
	{
		do {
			let encodedContent = try EncodedContent(
				serializedBytes: ffiMessage.message.content)
			if encodedContent.type == ContentTypeGroupUpdated
				&& ffiMessage.message.kind != .membershipChange
			{
				throw DecodedMessageError.decodeError(
					"Error decoding group membership change")
			}
			// Decode the content once during creation
			let decodedContent: Any = try encodedContent.decoded()

			let childMessages = try ffiMessage.reactions.map { reaction in
				let encodedContent = try EncodedContent(
					serializedBytes: reaction.content)
				// Decode the content once during creation
				let decodedContent: Any = try encodedContent.decoded()
				return DecodedMessage(
					ffiMessage: reaction, decodedContent: decodedContent,
					childMessages: nil)
			}

			return DecodedMessage(
				ffiMessage: ffiMessage.message, decodedContent: decodedContent,
				childMessages: childMessages)
		} catch {
			print("Error creating Message: \(error)")
			return nil
		}
	}
}
