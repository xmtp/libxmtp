import Foundation
import LibXMTP

enum MessageError: Error {
	case decodeError(String)
}

public enum MessageDeliveryStatus: String, RawRepresentable, Sendable {
	case all,
		published,
		unpublished,
		failed
}

public enum SortDirection {
	case descending, ascending
}

public struct Message: Identifiable {
	let client: Client
	let ffiMessage: FfiMessage

	init(client: Client, ffiMessage: FfiMessage) {
		self.client = client
		self.ffiMessage = ffiMessage
	}

	public var id: String {
		return ffiMessage.id.toHex
	}

	var convoId: String {
		return ffiMessage.convoId.toHex
	}

	var senderInboxId: String {
		return ffiMessage.senderInboxId
	}

	var sentAt: Date {
		return Date(
			timeIntervalSince1970: TimeInterval(ffiMessage.sentAtNs)
				/ 1_000_000_000)
	}

	var deliveryStatus: MessageDeliveryStatus {
		switch ffiMessage.deliveryStatus {
		case .unpublished:
			return .unpublished
		case .published:
			return .published
		case .failed:
			return .failed
		}
	}

	public func decode() throws -> DecodedMessage {
		do {
			let encodedContent = try EncodedContent(
				serializedData: ffiMessage.content)

			let decodedMessage = DecodedMessage(
				id: id,
				client: client,
				topic: Topic.groupMessage(convoId).description,
				encodedContent: encodedContent,
				senderAddress: senderInboxId,
				sent: sentAt,
				deliveryStatus: deliveryStatus
			)

			if decodedMessage.encodedContent.type == ContentTypeGroupUpdated
				&& ffiMessage.kind != .membershipChange
			{
				throw MessageError.decodeError(
					"Error decoding group membership change")
			}

			return decodedMessage
		} catch {
			throw MessageError.decodeError(
				"Error decoding message: \(error.localizedDescription)")
		}
	}

	public func decodeOrNull() -> DecodedMessage? {
		do {
			return try decode()
		} catch {
			print("MESSAGE: discarding message that failed to decode", error)
			return nil
		}
	}
}
