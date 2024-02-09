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
	let callback: (DecodedMessage) -> Void

	init(client: Client, _ callback: @escaping (DecodedMessage) -> Void) {
		self.client = client
		self.callback = callback
	}

	func onMessage(message: LibXMTP.FfiMessage) {
		do {
			try callback(message.fromFFI(client: client))
		} catch {
			print("Error onMessage \(error)")
		}
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

	public func sync() async throws {
		try await ffiGroup.sync()
	}

	public static func == (lhs: Group, rhs: Group) -> Bool {
		lhs.id == rhs.id
	}

	public func hash(into hasher: inout Hasher) {
		id.hash(into: &hasher)
	}

	public var memberAddresses: [String] {
		do {
			return try ffiGroup.listMembers().map(\.fromFFI.accountAddress)
		} catch {
			return []
		}
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

	public func send<T>(content: T, options: SendOptions? = nil) async throws {
		func encode<Codec: ContentCodec>(codec: Codec, content: Any) throws -> EncodedContent {
			if let content = content as? Codec.T {
				return try codec.encode(content: content, client: client)
			} else {
				throw CodecError.invalidContent
			}
		}

		let codec = client.codecRegistry.find(for: options?.contentType)
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

		try await ffiGroup.send(contentBytes: encoded.serializedData())
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
							continuation.yield(message)
						}
					)
				} catch {
					print("STREAM ERR: \(error)")
				}
			}
		}
	}

	public func messages(before: Date? = nil, after: Date? = nil, limit: Int? = nil) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(sentBeforeNs: nil, sentAfterNs: nil, limit: nil)

		if let before {
			options.sentBeforeNs = Int64(before.millisecondsSinceEpoch)
		}

		if let after {
			options.sentAfterNs = Int64(after.millisecondsSinceEpoch)
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let messages = try ffiGroup.findMessages(opts: options)

		return try messages.map { ffiMessage in
			try ffiMessage.fromFFI(client: client)
		}
	}
	
	public func decryptedMessages(before: Date? = nil, after: Date? = nil, limit: Int? = nil) async throws -> [DecryptedMessage] {
		var options = FfiListMessagesOptions(sentBeforeNs: nil, sentAfterNs: nil, limit: nil)

		if let before {
			options.sentBeforeNs = Int64(before.millisecondsSinceEpoch)
		}

		if let after {
			options.sentAfterNs = Int64(after.millisecondsSinceEpoch)
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let messages = try ffiGroup.findMessages(opts: options)

		return try messages.map { ffiMessage in
			try ffiMessage.fromFFIDecrypted(client: client)
		}
	}
}
