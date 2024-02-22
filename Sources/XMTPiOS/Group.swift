//
//  Group.swift
//
//
//  Created by Pat Nakajima on 2/1/24.
//

import Foundation
import LibXMTP

final class MessageCallback: FfiMessageCallback {
	let client: Client
	let callback: (LibXMTP.FfiMessage) -> Void

	init(client: Client, _ callback: @escaping (LibXMTP.FfiMessage) -> Void) {
		self.client = client
		self.callback = callback
	}

	func onMessage(message: LibXMTP.FfiMessage) {
		callback(message)
	}
}

final class StreamHolder {
	var stream: FfiStreamCloser?
}

public struct Group: Identifiable, Equatable, Hashable {
	var ffiGroup: FfiGroup
	var client: Client
	let streamHolder = StreamHolder()

	struct Member {
		var ffiGroupMember: FfiGroupMember

		public var accountAddress: String {
			ffiGroupMember.accountAddress
		}
	}

	public var id: Data {
		ffiGroup.id()
	}
	
	func metadata() throws -> FfiGroupMetadata {
		return try ffiGroup.groupMetadata()
	}

	public func sync() async throws {
		try await ffiGroup.sync()
	}

	public static func == (lhs: Group, rhs: Group) -> Bool {
		lhs.id == rhs.id
	}

	public func hash(into hasher: inout Hasher) {
		id.hash(into: &hasher)
	}
	
	public func isActive() throws -> Bool {
		return try ffiGroup.isActive()
	}
	
	public func isAdmin() throws -> Bool {
		return try metadata().creatorAccountAddress().lowercased() == client.address.lowercased()
	}

	public func permissionLevel() throws -> GroupPermissions {
		return try metadata().policyType()
	}

	public func adminAddress() throws -> String {
		return try metadata().creatorAccountAddress()
	}
	
	public var memberAddresses: [String] {
		do {
			return try ffiGroup.listMembers().map(\.fromFFI.accountAddress)
		} catch {
			return []
		}
	}
	
	public var peerAddresses: [String] {
		var addresses = memberAddresses.map(\.localizedLowercase)
		if let index = addresses.firstIndex(of: client.address.localizedLowercase) {
			addresses.remove(at: index)
		}
		return addresses
	}

	public var createdAt: Date {
		Date(millisecondsSinceEpoch: ffiGroup.createdAtNs())
	}

	public func addMembers(addresses: [String]) async throws {
		try await ffiGroup.addMembers(accountAddresses: addresses)
	}

	public func removeMembers(addresses: [String]) async throws {
		try await ffiGroup.removeMembers(accountAddresses: addresses)
	}


	public func send<T>(content: T, options: SendOptions? = nil) async throws -> String {
		let preparedMessage = try await prepareMessage(content: content, options: options)
		return try await send(encodedContent: preparedMessage)
	}
	
	public func send(encodedContent: EncodedContent) async throws -> String {
		try await ffiGroup.send(contentBytes: encodedContent.serializedData())
		return id.toHex
	}
	
	public func prepareMessage<T>(content: T, options: SendOptions?) async throws -> EncodedContent {
		let codec = client.codecRegistry.find(for: options?.contentType)

		func encode<Codec: ContentCodec>(codec: Codec, content: Any) throws -> EncodedContent {
			if let content = content as? Codec.T {
				return try codec.encode(content: content, client: client)
			} else {
				throw CodecError.invalidContent
			}
		}

		var encoded = try encode(codec: codec, content: content)
		
		func fallback<Codec: ContentCodec>(codec: Codec, content: Any) throws -> String? {
			if let content = content as? Codec.T {
				return try codec.fallback(content: content)
			} else {
				throw CodecError.invalidContent
			}
		}
		
		if let fallback = try fallback(codec: codec, content: content) {
			encoded.fallback = fallback
		}
		
		if let compression = options?.compression {
			encoded = try encoded.compress(compression)
		}

		return encoded
	}

	public func endStream() {
		self.streamHolder.stream?.end()
	}

	public func streamMessages() -> AsyncThrowingStream<DecodedMessage, Error> {
		AsyncThrowingStream { continuation in
			Task.detached {
				do {
					self.streamHolder.stream = try await ffiGroup.stream(
						messageCallback: MessageCallback(client: self.client) { message in
							do {
								continuation.yield(try message.fromFFI(client: self.client))
							} catch {
								print("Error onMessage \(error)")
							}
						}
					)
				} catch {
					print("STREAM ERR: \(error)")
				}
			}
		}
	}
	
	public func streamDecryptedMessages() -> AsyncThrowingStream<DecryptedMessage, Error> {
		AsyncThrowingStream { continuation in
			Task.detached {
				do {
					self.streamHolder.stream = try await ffiGroup.stream(
						messageCallback: MessageCallback(client: self.client) { message in
							do {
								continuation.yield(try message.fromFFIDecrypted(client: self.client))
							} catch {
								print("Error onMessage \(error)")
							}
						}
					)
				} catch {
					print("STREAM ERR: \(error)")
				}
			}
		}
	}

	public func messages(before: Date? = nil, after: Date? = nil, limit: Int? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(sentBeforeNs: nil, sentAfterNs: nil, limit: nil)

		if let before {
			options.sentBeforeNs = Int64(before.millisecondsSinceEpoch * 1_000_000)
		}

		if let after {
			options.sentAfterNs = Int64(after.millisecondsSinceEpoch * 1_000_000)
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let messages = try ffiGroup.findMessages(opts: options).map { ffiMessage in
			try ffiMessage.fromFFI(client: client)
		}

		switch direction {
		case .ascending:
			return messages
		default:
			return messages.reversed()
		}
	}
	
	public func decryptedMessages(before: Date? = nil, after: Date? = nil, limit: Int? = nil, direction: PagingInfoSortDirection? = .descending) async throws -> [DecryptedMessage] {
		var options = FfiListMessagesOptions(sentBeforeNs: nil, sentAfterNs: nil, limit: nil)

		if let before {
			options.sentBeforeNs = Int64(before.millisecondsSinceEpoch * 1_000_000)
		}

		if let after {
			options.sentAfterNs = Int64(after.millisecondsSinceEpoch * 1_000_000)
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let messages = try ffiGroup.findMessages(opts: options).map { ffiMessage in
			try ffiMessage.fromFFIDecrypted(client: client)
		}
		
		switch direction {
		case .ascending:
			return messages
		default:
			return messages.reversed()
		}

	}
}
