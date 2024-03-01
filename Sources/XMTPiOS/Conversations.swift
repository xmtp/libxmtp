import Foundation
import LibXMTP

public enum ConversationError: Error, CustomStringConvertible {
	case recipientNotOnNetwork, recipientIsSender, v1NotSupported(String)

	public var description: String {
		switch self {
		case .recipientIsSender:
			return "ConversationError.recipientIsSender: Recipient cannot be sender"
		case .recipientNotOnNetwork:
			return "ConversationError.recipientNotOnNetwork: Recipient is not on network"
		case .v1NotSupported(let str):
			return "ConversationError.v1NotSupported: V1 does not support: \(str)"
		}
	}
}

public enum GroupError: Error, CustomStringConvertible {
	case alphaMLSNotEnabled, emptyCreation, memberCannotBeSelf, memberNotRegistered([String]), groupsRequireMessagePassed, notSupportedByGroups

	public var description: String {
		switch self {
		case .alphaMLSNotEnabled:
			return "GroupError.alphaMLSNotEnabled"
		case .emptyCreation:
			return "GroupError.emptyCreation you cannot create an empty group"
		case .memberCannotBeSelf:
			return "GroupError.memberCannotBeSelf you cannot add yourself to a group"
		case .memberNotRegistered(let array):
			return "GroupError.memberNotRegistered members not registered: \(array.joined(separator: ", "))"
		case .groupsRequireMessagePassed:
			return "GroupError.groupsRequireMessagePassed you cannot call this method without passing a message instead of an envelope"
		case .notSupportedByGroups:
			return "GroupError.notSupportedByGroups this method is not supported by groups"
		}
	}
}

final class GroupStreamCallback: FfiConversationCallback {
	let client: Client
	let callback: (Group) -> Void

	init(client: Client, callback: @escaping (Group) -> Void) {
		self.client = client
		self.callback = callback
	}

	func onConversation(conversation: FfiGroup) {
		self.callback(conversation.fromFFI(client: client))
	}
}

/// Handles listing and creating Conversations.
public actor Conversations {
	var client: Client
	var conversationsByTopic: [String: Conversation] = [:]
	let streamHolder = StreamHolder()

	init(client: Client) {
		self.client = client
	}

	public func sync() async throws {
		guard let v3Client = client.v3Client else {
			return
		}

		try await v3Client.conversations().sync()
	}

	public func groups(createdAfter: Date? = nil, createdBefore: Date? = nil, limit: Int? = nil) async throws -> [Group] {
		guard let v3Client = client.v3Client else {
			return []
		}

		var options = FfiListConversationsOptions(createdAfterNs: nil, createdBeforeNs: nil, limit: nil)

		if let createdAfter {
			options.createdAfterNs = Int64(createdAfter.millisecondsSinceEpoch)
		}

		if let createdBefore {
			options.createdBeforeNs = Int64(createdBefore.millisecondsSinceEpoch)
		}

		if let limit {
			options.limit = Int64(limit)
		}

		return try await v3Client.conversations().list(opts: options).map { $0.fromFFI(client: client) }
	}

	public func streamGroups() async throws -> AsyncThrowingStream<Group, Error> {
		AsyncThrowingStream { continuation in
			Task {
				self.streamHolder.stream = try await self.client.v3Client?.conversations().stream(
					callback: GroupStreamCallback(client: self.client) { group in
						continuation.yield(group)
					}
				)
			}
		}
	}
	
	private func streamGroupConversations() -> AsyncThrowingStream<Conversation, Error> {
		AsyncThrowingStream { continuation in
			Task {
				self.streamHolder.stream = try await self.client.v3Client?.conversations().stream(
					callback: GroupStreamCallback(client: self.client) { group in
						continuation.yield(Conversation.group(group))
					}
				)
			}
		}
	}

	public func newGroup(with addresses: [String], permissions: GroupPermissions = .everyoneIsAdmin) async throws -> Group {
		guard let v3Client = client.v3Client else {
			throw GroupError.alphaMLSNotEnabled
		}

		if addresses.isEmpty {
			throw GroupError.emptyCreation
		}

		if addresses.first(where: { $0.lowercased() == client.address.lowercased() }) != nil {
			throw GroupError.memberCannotBeSelf
		}

		let erroredAddresses = try await withThrowingTaskGroup(of: (String?).self) { group in
			for address in addresses {
				group.addTask {
					if try await self.client.canMessageV3(address: address) {
						return nil
					} else {
						return address
					}
				}
			}

			var results: [String] = []
			for try await result in group {
				if let result {
					results.append(result)
				}
			}

			return results
		}

		if !erroredAddresses.isEmpty {
			throw GroupError.memberNotRegistered(erroredAddresses)
		}

		let group = try await v3Client.conversations().createGroup(accountAddresses: addresses, permissions: permissions).fromFFI(client: client)

		try await client.contacts.allowGroup(groupIds: [group.id])

		return group
	}

	/// Import a previously seen conversation.
	/// See Conversation.toTopicData()
	public func importTopicData(data: Xmtp_KeystoreApi_V1_TopicMap.TopicData) -> Conversation {
		let conversation: Conversation
		if !data.hasInvitation {
			let sentAt = Date(timeIntervalSince1970: TimeInterval(data.createdNs / 1_000_000_000))
			conversation = .v1(ConversationV1(client: client, peerAddress: data.peerAddress, sentAt: sentAt))
		} else {
			conversation = .v2(ConversationV2(
				topic: data.invitation.topic,
				keyMaterial: data.invitation.aes256GcmHkdfSha256.keyMaterial,
				context: data.invitation.context,
				peerAddress: data.peerAddress,
				client: client,
				createdAtNs: data.createdNs
			))
		}
		conversationsByTopic[conversation.topic] = conversation
		return conversation
	}

	public func listBatchMessages(topics: [String: Pagination?]) async throws -> [DecodedMessage] {
		let requests = topics.map { topic, page in
			makeQueryRequest(topic: topic, pagination: page)
		}
		/// The maximum number of requests permitted in a single batch call.
		let maxQueryRequestsPerBatch = 50
		let batches = requests.chunks(maxQueryRequestsPerBatch)
			.map { requests in BatchQueryRequest.with { $0.requests = requests } }
		var messages: [DecodedMessage] = []
		// TODO: consider using a task group here for parallel batch calls
		for batch in batches {
			messages += try await client.apiClient.batchQuery(request: batch)
				.responses.flatMap { res in
					res.envelopes.compactMap { envelope in
						let conversation = conversationsByTopic[envelope.contentTopic]
						if conversation == nil {
							print("discarding message, unknown conversation \(envelope)")
							return nil
						}
						do {
							return try conversation?.decode(envelope)
						} catch {
							print("discarding message, unable to decode \(envelope)")
							return nil
						}
					}
				}
		}
		return messages
	}

	public func listBatchDecryptedMessages(topics: [String: Pagination?]) async throws -> [DecryptedMessage] {
		let requests = topics.map { topic, page in
			makeQueryRequest(topic: topic, pagination: page)
		}
		/// The maximum number of requests permitted in a single batch call.
		let maxQueryRequestsPerBatch = 50
		let batches = requests.chunks(maxQueryRequestsPerBatch)
			.map { requests in BatchQueryRequest.with { $0.requests = requests } }
		var messages: [DecryptedMessage] = []
		// TODO: consider using a task group here for parallel batch calls
		for batch in batches {
			messages += try await client.apiClient.batchQuery(request: batch)
				.responses.flatMap { res in
					res.envelopes.compactMap { envelope in
						let conversation = conversationsByTopic[envelope.contentTopic]
						if conversation == nil {
							print("discarding message, unknown conversation \(envelope)")
							return nil
						}
						do {
							return try conversation?.decrypt(envelope)
						} catch {
							print("discarding message, unable to decode \(envelope)")
							return nil
						}
					}
				}
		}
		return messages
	}

	func streamAllV2Messages() async throws -> AsyncThrowingStream<DecodedMessage, Error> {
		return AsyncThrowingStream { continuation in
			Task {
				while true {
					var topics: [String] = [
						Topic.userInvite(client.address).description,
						Topic.userIntro(client.address).description,
					]

					for conversation in try await list() {
						topics.append(conversation.topic)
					}

					do {
						for try await envelope in client.subscribe(topics: topics) {
							if let conversation = conversationsByTopic[envelope.contentTopic] {
								let decoded = try conversation.decode(envelope)
								continuation.yield(decoded)
							} else if envelope.contentTopic.hasPrefix("/xmtp/0/invite-") {
								let conversation = try fromInvite(envelope: envelope)
								conversationsByTopic[conversation.topic] = conversation
								break // Break so we can resubscribe with the new conversation
							} else if envelope.contentTopic.hasPrefix("/xmtp/0/intro-") {
								let conversation = try fromIntro(envelope: envelope)
								conversationsByTopic[conversation.topic] = conversation
								let decoded = try conversation.decode(envelope)
								continuation.yield(decoded)
								break // Break so we can resubscribe with the new conversation
							} else {
								print("huh \(envelope)")
							}
						}
					} catch {
						continuation.finish(throwing: error)
					}
				}
			}
		}
	}
	
	public func streamAllGroupMessages() -> AsyncThrowingStream<DecodedMessage, Error> {
		AsyncThrowingStream { continuation in
			Task {
				do {
					self.streamHolder.stream = try await self.client.v3Client?.conversations().streamAllMessages(
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
	
	public func streamAllMessages(includeGroups: Bool = false) async throws -> AsyncThrowingStream<DecodedMessage, Error> {
		AsyncThrowingStream<DecodedMessage, Error> { continuation in
			@Sendable func forwardStreamToMerged(stream: AsyncThrowingStream<DecodedMessage, Error>) async {
				do {
					var iterator = stream.makeAsyncIterator()
					while let element = try await  iterator.next() {
						continuation.yield(element)
					}
					continuation.finish()
				} catch {
					continuation.finish(throwing: error)
				}
			}
			
			Task {
				await forwardStreamToMerged(stream: try streamAllV2Messages())
			}
			if (includeGroups) {
				Task {
					await forwardStreamToMerged(stream: streamAllGroupMessages())
				}
			}
		}
	}
	
	public func streamAllGroupDecryptedMessages() -> AsyncThrowingStream<DecryptedMessage, Error> {
		AsyncThrowingStream { continuation in
			Task {
				do {
					self.streamHolder.stream = try await self.client.v3Client?.conversations().streamAllMessages(
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
	
	public func streamAllDecryptedMessages(includeGroups: Bool = false) -> AsyncThrowingStream<DecryptedMessage, Error> {
		AsyncThrowingStream<DecryptedMessage, Error> { continuation in
			@Sendable func forwardStreamToMerged(stream: AsyncThrowingStream<DecryptedMessage, Error>) async {
				do {
					var iterator = stream.makeAsyncIterator()
					while let element = try await  iterator.next() {
						continuation.yield(element)
					}
					continuation.finish()
				} catch {
					continuation.finish(throwing: error)
				}
			}
			
			Task {
				await forwardStreamToMerged(stream: try streamAllV2DecryptedMessages())
			}
			if (includeGroups) {
				Task {
					await forwardStreamToMerged(stream: streamAllGroupDecryptedMessages())
				}
			}
		}
	}


	func streamAllV2DecryptedMessages() async throws -> AsyncThrowingStream<DecryptedMessage, Error> {
		return AsyncThrowingStream { continuation in
			Task {
				while true {
					var topics: [String] = [
						Topic.userInvite(client.address).description,
						Topic.userIntro(client.address).description,
					]

					for conversation in try await list() {
						topics.append(conversation.topic)
					}

					do {
						for try await envelope in client.subscribe(topics: topics) {
							if let conversation = conversationsByTopic[envelope.contentTopic] {
								let decoded = try conversation.decrypt(envelope)
								continuation.yield(decoded)
							} else if envelope.contentTopic.hasPrefix("/xmtp/0/invite-") {
								let conversation = try fromInvite(envelope: envelope)
								conversationsByTopic[conversation.topic] = conversation
								break // Break so we can resubscribe with the new conversation
							} else if envelope.contentTopic.hasPrefix("/xmtp/0/intro-") {
								let conversation = try fromIntro(envelope: envelope)
								conversationsByTopic[conversation.topic] = conversation
								let decoded = try conversation.decrypt(envelope)
								continuation.yield(decoded)
								break // Break so we can resubscribe with the new conversation
							} else {
								print("huh \(envelope)")
							}
						}
					} catch {
						continuation.finish(throwing: error)
					}
				}
			}
		}
	}

	public func fromInvite(envelope: Envelope) throws -> Conversation {
		let sealedInvitation = try SealedInvitation(serializedData: envelope.message)
		let unsealed = try sealedInvitation.v1.getInvitation(viewer: client.keys)

		return try .v2(ConversationV2.create(client: client, invitation: unsealed, header: sealedInvitation.v1.header))
	}

	public func fromIntro(envelope: Envelope) throws -> Conversation {
		let messageV1 = try MessageV1.fromBytes(envelope.message)
		let senderAddress = try messageV1.header.sender.walletAddress
		let recipientAddress = try messageV1.header.recipient.walletAddress

		let peerAddress = client.address == senderAddress ? recipientAddress : senderAddress
		let conversationV1 = ConversationV1(client: client, peerAddress: peerAddress, sentAt: messageV1.sentAt)

		return .v1(conversationV1)
	}

	private func findExistingConversation(with peerAddress: String, conversationID: String?) -> Conversation? {
		return conversationsByTopic.first(where: { $0.value.peerAddress == peerAddress &&
				(($0.value.conversationID ?? "") == (conversationID ?? ""))
		})?.value
	}

	public func newConversation(with peerAddress: String, context: InvitationV1.Context? = nil) async throws -> Conversation {
		if peerAddress.lowercased() == client.address.lowercased() {
			throw ConversationError.recipientIsSender
		}
		print("\(client.address) starting conversation with \(peerAddress)")
		if let existing = findExistingConversation(with: peerAddress, conversationID: context?.conversationID) {
			return existing
		}

		guard let contact = try await client.contacts.find(peerAddress) else {
			throw ConversationError.recipientNotOnNetwork
		}

		_ = try await list() // cache old conversations and check again
		if let existing = findExistingConversation(with: peerAddress, conversationID: context?.conversationID) {
			return existing
		}

		// We don't have an existing conversation, make a v2 one
		let recipient = try contact.toSignedPublicKeyBundle()
		let invitation = try InvitationV1.createDeterministic(
			sender: client.keys,
			recipient: recipient,
			context: context
		)
		let sealedInvitation = try await sendInvitation(recipient: recipient, invitation: invitation, created: Date())
		let conversationV2 = try ConversationV2.create(client: client, invitation: invitation, header: sealedInvitation.v1.header)

		try await client.contacts.allow(addresses: [peerAddress])

		let conversation: Conversation = .v2(conversationV2)
		conversationsByTopic[conversation.topic] = conversation
		return conversation
	}

	public func stream() -> AsyncThrowingStream<Conversation, Error> {
		AsyncThrowingStream { continuation in
			Task {
				var streamedConversationTopics: Set<String> = []

				for try await envelope in client.subscribe(topics: [.userIntro(client.address), .userInvite(client.address)]) {
					if envelope.contentTopic == Topic.userIntro(client.address).description {
						let conversationV1 = try fromIntro(envelope: envelope)

						if streamedConversationTopics.contains(conversationV1.topic.description) {
							continue
						}

						streamedConversationTopics.insert(conversationV1.topic.description)
						continuation.yield(conversationV1)
					}

					if envelope.contentTopic == Topic.userInvite(client.address).description {
						let conversationV2 = try fromInvite(envelope: envelope)

						if streamedConversationTopics.contains(conversationV2.topic) {
							continue
						}

						streamedConversationTopics.insert(conversationV2.topic)
						continuation.yield(conversationV2)
					}
				}
			}
		}
	}

   public func streamAll() -> AsyncThrowingStream<Conversation, Error> {
	   AsyncThrowingStream<Conversation, Error> { continuation in
		   @Sendable func forwardStreamToMerged(stream: AsyncThrowingStream<Conversation, Error>) async {
			   do {
				   var iterator = stream.makeAsyncIterator()
				   while let element = try await  iterator.next() {
					   continuation.yield(element)
				   }
				   continuation.finish()
			   } catch {
				   continuation.finish(throwing: error)
			   }
		   }
		   
		   Task {
			   await forwardStreamToMerged(stream: stream())
		   }
		   Task {
			   await forwardStreamToMerged(stream: streamGroupConversations())
		   }
	   }
   }

	private func makeConversation(from sealedInvitation: SealedInvitation) throws -> ConversationV2 {
		let unsealed = try sealedInvitation.v1.getInvitation(viewer: client.keys)
		let conversation = try ConversationV2.create(client: client, invitation: unsealed, header: sealedInvitation.v1.header)

		return conversation
	}

	public func list(includeGroups: Bool = false) async throws -> [Conversation] {
		if (includeGroups) {
			try await sync()
			let groups = try await groups()

			groups.forEach { group in
				conversationsByTopic[group.id.toHex] = Conversation.group(group)
			}
		}
		var newConversations: [Conversation] = []
		let mostRecent = conversationsByTopic.values.max { a, b in
			a.createdAt < b.createdAt
		}
		let pagination = Pagination(after: mostRecent?.createdAt)
		do {
			let seenPeers = try await listIntroductionPeers(pagination: pagination)
			for (peerAddress, sentAt) in seenPeers {
				newConversations.append(
					Conversation.v1(
						ConversationV1(
							client: client,
							peerAddress: peerAddress,
							sentAt: sentAt
						)
					)
				)
			}
		} catch {
			print("Error loading introduction peers: \(error)")
		}

		for sealedInvitation in try await listInvitations(pagination: pagination) {
			do {
				try newConversations.append(
					Conversation.v2(makeConversation(from: sealedInvitation))
				)
			} catch {
				print("Error loading invitations: \(error)")
			}
		}

		newConversations
			.filter { $0.peerAddress != client.address && Topic.isValidTopic(topic: $0.topic) }
			.forEach { conversationsByTopic[$0.topic] = $0 }

		// TODO(perf): use DB to persist + sort
		return conversationsByTopic.values.sorted { a, b in
			a.createdAt < b.createdAt
		}
	}
	
	public func getHmacKeys(request: Xmtp_KeystoreApi_V1_GetConversationHmacKeysRequest? = nil) -> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse {
		let thirtyDayPeriodsSinceEpoch = Int(Date().timeIntervalSince1970) / (60 * 60 * 24 * 30)
		var hmacKeysResponse = Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse()
		
		var topics = conversationsByTopic

		if let requestTopics = request?.topics, !requestTopics.isEmpty {
			topics = topics.filter { requestTopics.contains($0.key) }
		}
		
		for (topic, conversation) in topics {
			guard let keyMaterial = conversation.keyMaterial else { continue }
			
			var hmacKeys = Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse.HmacKeys()

			for period in (thirtyDayPeriodsSinceEpoch - 1)...(thirtyDayPeriodsSinceEpoch + 1) {
				let info = "\(period)-\(client.address)"
				do {
					let hmacKey = try Crypto.deriveKey(secret: keyMaterial, nonce: Data(), info: Data(info.utf8))
					var hmacKeyData = Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse.HmacKeyData()
					hmacKeyData.hmacKey = hmacKey
					hmacKeyData.thirtyDayPeriodsSinceEpoch = Int32(period)
					hmacKeys.values.append(hmacKeyData)
				} catch {
					print("Error calculating HMAC key for topic \(topic): \(error)")
				}
			}
			hmacKeysResponse.hmacKeys[topic] = hmacKeys
		}
		
		return hmacKeysResponse
	}

	private func listIntroductionPeers(pagination: Pagination?) async throws -> [String: Date] {
		let envelopes = try await client.apiClient.query(
			topic: .userIntro(client.address),
			pagination: pagination
		).envelopes

		let messages = envelopes.compactMap { envelope in
			do {
				let message = try MessageV1.fromBytes(envelope.message)

				// Attempt to decrypt, just to make sure we can
				_ = try message.decrypt(with: client.privateKeyBundleV1)

				return message
			} catch {
				return nil
			}
		}

		var seenPeers: [String: Date] = [:]
		for message in messages {
			guard let recipientAddress = message.recipientAddress,
			      let senderAddress = message.senderAddress
			else {
				continue
			}

			let sentAt = message.sentAt
			let peerAddress = recipientAddress == client.address ? senderAddress : recipientAddress

			guard let existing = seenPeers[peerAddress] else {
				seenPeers[peerAddress] = sentAt
				continue
			}

			if existing > sentAt {
				seenPeers[peerAddress] = sentAt
			}
		}

		return seenPeers
	}

	private func listInvitations(pagination: Pagination?) async throws -> [SealedInvitation] {
		var envelopes = try await client.apiClient.envelopes(
			topic: Topic.userInvite(client.address).description,
			pagination: pagination
		)

		return envelopes.compactMap { envelope in
			// swiftlint:disable no_optional_try
			try? SealedInvitation(serializedData: envelope.message)
			// swiftlint:enable no_optional_try
		}
	}

	func sendInvitation(recipient: SignedPublicKeyBundle, invitation: InvitationV1, created: Date) async throws -> SealedInvitation {
		let sealed = try SealedInvitation.createV1(
			sender: client.keys,
			recipient: recipient,
			created: created,
			invitation: invitation
		)

		let peerAddress = try recipient.walletAddress

		try await client.publish(envelopes: [
			Envelope(topic: .userInvite(client.address), timestamp: created, message: sealed.serializedData()),
			Envelope(topic: .userInvite(peerAddress), timestamp: created, message: sealed.serializedData()),
		])

		return sealed
	}
}
