//
//  MessageV3.swift
//  
//
//  Created by Naomi Plasterer on 4/10/24.
//

import Foundation
import LibXMTP

enum MessageV3Error: Error {
	case decodeError(String)
}

public struct MessageV3: Identifiable {
	let client: Client
	let ffiMessage: FfiMessage
	
	init(client: Client, ffiMessage: FfiMessage) {
		self.client = client
		self.ffiMessage = ffiMessage
	}
	
	public var id: Data {
		return ffiMessage.id
	}
	
	var convoId: Data {
		return ffiMessage.convoId
	}
	
	var senderAddress: String {
		return ffiMessage.addrFrom
	}
	
	var sentAt: Date {
		return Date(timeIntervalSince1970: TimeInterval(ffiMessage.sentAtNs) / 1_000_000_000)
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
			let encodedContent = try EncodedContent(serializedData: ffiMessage.content)

			let decodedMessage = DecodedMessage(
				id: id.toHex,
				client: client,
				topic: Topic.groupMessage(convoId.toHex).description,
				encodedContent: encodedContent,
				senderAddress: senderAddress,
				sent: sentAt,
				deliveryStatus: deliveryStatus
			)
			
			if decodedMessage.encodedContent.type == ContentTypeGroupMembershipChanged && ffiMessage.kind != .membershipChange {
				throw MessageV3Error.decodeError("Error decoding group membership change")
		 }
			
			return decodedMessage
		} catch {
			throw MessageV3Error.decodeError("Error decoding message: \(error.localizedDescription)")
		}
	}
	
	public func decodeOrNull() -> DecodedMessage? {
		do {
			return try decode()
		} catch {
			print("MESSAGE_V3: discarding message that failed to decode", error)
			return nil
		}
	}
	
	public func decryptOrNull() -> DecryptedMessage? {
		do {
			return try decrypt()
		} catch {
			print("MESSAGE_V3: discarding message that failed to decrypt", error)
			return nil
		}
	}
	
	public func decrypt() throws -> DecryptedMessage {
		let encodedContent = try EncodedContent(serializedData: ffiMessage.content)

		let decrytedMessage =  DecryptedMessage(
			id: id.toHex,
			encodedContent: encodedContent,
			senderAddress: senderAddress,
			sentAt: Date(),
			topic: Topic.groupMessage(convoId.toHex).description,
			deliveryStatus: deliveryStatus
		)
		
		if decrytedMessage.encodedContent.type == ContentTypeGroupMembershipChanged && ffiMessage.kind != .membershipChange {
			throw MessageV3Error.decodeError("Error decoding group membership change")
		}
		
		return decrytedMessage
	}
}
