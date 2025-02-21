import Foundation
import LibXMTP

public enum ConversationError: Error, CustomStringConvertible, LocalizedError {
	case memberCannotBeSelf
	case memberNotRegistered([String])
	case groupsRequireMessagePassed, notSupportedByGroups, streamingFailure

	public var description: String {
		switch self {
		case .memberCannotBeSelf:
			return
				"GroupError.memberCannotBeSelf you cannot add yourself to a group"
		case .memberNotRegistered(let array):
			return
				"GroupError.memberNotRegistered members not registered: \(array.joined(separator: ", "))"
		case .groupsRequireMessagePassed:
			return
				"GroupError.groupsRequireMessagePassed you cannot call this method without passing a message instead of an envelope"
		case .notSupportedByGroups:
			return
				"GroupError.notSupportedByGroups this method is not supported by groups"
		case .streamingFailure:
			return "GroupError.streamingFailure a stream has failed"
		}
	}

	public var errorDescription: String? {
		return description
	}
}

public enum ConversationType {
	case all, groups, dms
}

final class ConversationStreamCallback: FfiConversationCallback {
	func onError(error: LibXMTP.FfiSubscribeError) {
		print("Error ConversationStreamCallback \(error)")
	}

	let callback: (FfiConversation) -> Void

	init(callback: @escaping (FfiConversation) -> Void) {
		self.callback = callback
	}

	func onConversation(conversation: FfiConversation) {
		self.callback(conversation)
	}
}

actor FfiStreamActor {
	private var ffiStream: FfiStreamCloser?

	func setFfiStream(_ stream: FfiStreamCloser?) {
		ffiStream = stream
	}

	func endStream() {
		ffiStream?.end()
	}
}

/// Handles listing and creating Conversations.
public actor Conversations {
	var client: Client
	var ffiConversations: FfiConversations
	var ffiClient: FfiXmtpClient

	init(
		client: Client, ffiConversations: FfiConversations,
		ffiClient: FfiXmtpClient
	) {
		self.client = client
		self.ffiConversations = ffiConversations
		self.ffiClient = ffiClient
	}

	public func findGroup(groupId: String) throws -> Group? {
		do {
			return Group(
				ffiGroup: try ffiClient.conversation(
					conversationId: groupId.hexToData),
				client: client)
		} catch {
			return nil
		}
	}

	public func findConversation(conversationId: String) async throws
		-> Conversation?
	{
		do {
			let conversation = try ffiClient.conversation(
				conversationId: conversationId.hexToData)
			return try await conversation.toConversation(client: client)
		} catch {
			return nil
		}
	}

	public func findConversationByTopic(topic: String) async throws
		-> Conversation?
	{
		do {
			let regexPattern = #"/xmtp/mls/1/g-(.*?)/proto"#
			if let regex = try? NSRegularExpression(pattern: regexPattern) {
				let range = NSRange(location: 0, length: topic.utf16.count)
				if let match = regex.firstMatch(
					in: topic, options: [], range: range)
				{
					let conversationId = (topic as NSString).substring(
						with: match.range(at: 1))
					let conversation = try ffiClient.conversation(
						conversationId: conversationId.hexToData)
					return try await conversation.toConversation(client: client)
				}
			}
		} catch {
			return nil
		}
		return nil
	}

	public func findDmByInboxId(inboxId: String) throws -> Dm? {
		do {
			let conversation = try ffiClient.dmConversation(
				targetInboxId: inboxId)
			return Dm(
				ffiConversation: conversation, client: client)
		} catch {
			return nil
		}
	}

	public func findDmByAddress(address: String) async throws -> Dm? {
		guard
			let inboxId = try await client.inboxIdFromAddress(address: address)
		else {
			throw ClientError.creationError("No inboxId present")
		}
		return try findDmByInboxId(inboxId: inboxId)
	}

	public func findMessage(messageId: String) throws -> Message? {
		do {
			return Message.create(
				ffiMessage: try ffiClient.message(
					messageId: messageId.hexToData))
		} catch {
			return nil
		}
	}

	public func sync() async throws {
		try await ffiConversations.sync()
	}
	public func syncAllConversations(consentStates: [ConsentState]? = nil)
		async throws -> UInt32
	{
		return try await ffiConversations.syncAllConversations(
			consentStates: consentStates?.toFFI)
	}

	public func listGroups(
		createdAfter: Date? = nil, createdBefore: Date? = nil,
		limit: Int? = nil,
		consentStates: [ConsentState]? = nil
	) throws -> [Group] {
		var options = FfiListConversationsOptions(
			createdAfterNs: nil, createdBeforeNs: nil, limit: nil,
			consentStates: consentStates?.toFFI, includeDuplicateDms: false)
		if let createdAfter {
			options.createdAfterNs = Int64(createdAfter.millisecondsSinceEpoch)
		}
		if let createdBefore {
			options.createdBeforeNs = Int64(
				createdBefore.millisecondsSinceEpoch)
		}
		if let limit {
			options.limit = Int64(limit)
		}
		let conversations = try ffiConversations.listGroups(
			opts: options)

		return conversations.map {
			$0.groupFromFFI(client: client)
		}
	}

	public func listDms(
		createdAfter: Date? = nil, createdBefore: Date? = nil,
		limit: Int? = nil,
		consentStates: [ConsentState]? = nil
	) throws -> [Dm] {
		var options = FfiListConversationsOptions(
			createdAfterNs: nil, createdBeforeNs: nil, limit: nil,
			consentStates: consentStates?.toFFI, includeDuplicateDms: false)
		if let createdAfter {
			options.createdAfterNs = Int64(createdAfter.millisecondsSinceEpoch)
		}
		if let createdBefore {
			options.createdBeforeNs = Int64(
				createdBefore.millisecondsSinceEpoch)
		}
		if let limit {
			options.limit = Int64(limit)
		}
		let conversations = try ffiConversations.listDms(
			opts: options)

		return conversations.map {
			$0.dmFromFFI(client: client)
		}
	}

	public func list(
		createdAfter: Date? = nil, createdBefore: Date? = nil,
		limit: Int? = nil,
		consentStates: [ConsentState]? = nil
	) async throws -> [Conversation] {
		var options = FfiListConversationsOptions(
			createdAfterNs: nil, createdBeforeNs: nil, limit: nil,
			consentStates: consentStates?.toFFI, includeDuplicateDms: false)
		if let createdAfter {
			options.createdAfterNs = Int64(createdAfter.millisecondsSinceEpoch)
		}
		if let createdBefore {
			options.createdBeforeNs = Int64(
				createdBefore.millisecondsSinceEpoch)
		}
		if let limit {
			options.limit = Int64(limit)
		}
		let ffiConversations = try ffiConversations.list(
			opts: options)

		var conversations: [Conversation] = []
		for conversation in ffiConversations {
			let conversation = try await conversation.toConversation(
				client: client)
			conversations.append(conversation)
		}
		return conversations
	}

	public func stream(type: ConversationType = .all) -> AsyncThrowingStream<
		Conversation, Error
	> {
		AsyncThrowingStream { continuation in
			let ffiStreamActor = FfiStreamActor()
			let conversationCallback = ConversationStreamCallback {
				conversation in
				Task {
					guard !Task.isCancelled else {
						continuation.finish()
						return
					}
					do {
						let conversationType =
							try await conversation.conversationType()
						if conversationType == .dm {
							continuation.yield(
								Conversation.dm(
									conversation.dmFromFFI(client: self.client))
							)
						} else if conversationType == .group {
							continuation.yield(
								Conversation.group(
									conversation.groupFromFFI(
										client: self.client))
							)
						}
					} catch {
						print("Error processing conversation type: \(error)")
					}
				}
			}

			let task = Task {
				let stream: FfiStreamCloser
				switch type {
				case .groups:
					stream = await ffiConversations.streamGroups(
						callback: conversationCallback)
				case .all:
					stream = await ffiConversations.stream(
						callback: conversationCallback)
				case .dms:
					stream = await ffiConversations.streamDms(
						callback: conversationCallback)
				}
				await ffiStreamActor.setFfiStream(stream)
				continuation.onTermination = { @Sendable reason in
					Task {
						await ffiStreamActor.endStream()
					}
				}
			}

			continuation.onTermination = { @Sendable reason in
				task.cancel()
				Task {
					await ffiStreamActor.endStream()
				}
			}
		}
	}

	public func newConversation(
		with peerAddress: String,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Conversation {
		let dm = try await findOrCreateDm(
			with: peerAddress,
			disappearingMessageSettings: disappearingMessageSettings)
		return Conversation.dm(dm)
	}

	public func findOrCreateDm(
		with peerAddress: String,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Dm {
		if peerAddress.lowercased() == client.address.lowercased() {
			throw ConversationError.memberCannotBeSelf
		}
		let canMessage = try await self.client.canMessage(
			address: peerAddress)
		if !canMessage {
			throw ConversationError.memberNotRegistered([peerAddress])
		}

		let dm =
			try await ffiConversations
			.findOrCreateDm(
				accountAddress: peerAddress.lowercased(),
				opts: FfiCreateDmOptions(
					messageDisappearingSettings: FfiMessageDisappearingSettings(
						fromNs: disappearingMessageSettings?
							.disappearStartingAtNs ?? 0,
						inNs: disappearingMessageSettings?.retentionDurationInNs
							?? 0)))

		return dm.dmFromFFI(client: client)
	}

	public func newConversationWithInboxId(
		with peerInboxId: String,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Conversation {
		let dm = try await findOrCreateDmWithInboxId(
			with: peerInboxId,
			disappearingMessageSettings: disappearingMessageSettings)
		return Conversation.dm(dm)
	}

	public func findOrCreateDmWithInboxId(
		with peerInboxId: String,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	)
		async throws -> Dm
	{
		if peerInboxId.lowercased() == client.inboxID.lowercased() {
			throw ConversationError.memberCannotBeSelf
		}

		let dm =
			try await ffiConversations
			.findOrCreateDmByInboxId(
				inboxId: peerInboxId,
				opts: FfiCreateDmOptions(
					messageDisappearingSettings: FfiMessageDisappearingSettings(
						fromNs: disappearingMessageSettings?
							.disappearStartingAtNs ?? 0,
						inNs: disappearingMessageSettings?.retentionDurationInNs
							?? 0)))
		return dm.dmFromFFI(client: client)

	}

	public func newGroup(
		with addresses: [String],
		permissions: GroupPermissionPreconfiguration = .allMembers,
		name: String = "",
		imageUrlSquare: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternal(
			with: addresses,
			permissions:
				GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
					option: permissions),
			name: name,
			imageUrlSquare: imageUrlSquare,
			description: description,
			permissionPolicySet: nil,
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	public func newGroupCustomPermissions(
		with addresses: [String],
		permissionPolicySet: PermissionPolicySet,
		name: String = "",
		imageUrlSquare: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternal(
			with: addresses,
			permissions: FfiGroupPermissionsOptions.customPolicy,
			name: name,
			imageUrlSquare: imageUrlSquare,
			description: description,
			permissionPolicySet: PermissionPolicySet.toFfiPermissionPolicySet(
				permissionPolicySet),
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	private func newGroupInternal(
		with addresses: [String],
		permissions: FfiGroupPermissionsOptions = .default,
		name: String = "",
		imageUrlSquare: String = "",
		description: String = "",
		permissionPolicySet: FfiPermissionPolicySet? = nil,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		if addresses.first(where: {
			$0.lowercased() == client.address.lowercased()
		}) != nil {
			throw ConversationError.memberCannotBeSelf
		}
		let addressMap = try await self.client.canMessage(addresses: addresses)
		let unregisteredAddresses =
			addressMap
			.filter { !$0.value }
			.map { $0.key }

		if !unregisteredAddresses.isEmpty {
			throw ConversationError.memberNotRegistered(unregisteredAddresses)
		}

		let group = try await ffiConversations.createGroup(
			accountAddresses: addresses,
			opts: FfiCreateGroupOptions(
				permissions: permissions,
				groupName: name,
				groupImageUrlSquare: imageUrlSquare,
				groupDescription: description,
				customPermissionPolicySet: permissionPolicySet,
				messageDisappearingSettings: FfiMessageDisappearingSettings(
					fromNs: disappearingMessageSettings?
						.disappearStartingAtNs ?? 0,
					inNs: disappearingMessageSettings?
						.retentionDurationInNs ?? 0
				)
			)
		).groupFromFFI(client: client)
		return group
	}

	public func newGroupWithInboxIds(
		with inboxIds: [String],
		permissions: GroupPermissionPreconfiguration = .allMembers,
		name: String = "",
		imageUrlSquare: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternalWithInboxIds(
			with: inboxIds,
			permissions:
				GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
					option: permissions),
			name: name,
			imageUrlSquare: imageUrlSquare,
			description: description,
			permissionPolicySet: nil,
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	public func newGroupCustomPermissionsWithInboxIds(
		with inboxIds: [String],
		permissionPolicySet: PermissionPolicySet,
		name: String = "",
		imageUrlSquare: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternalWithInboxIds(
			with: inboxIds,
			permissions: FfiGroupPermissionsOptions.customPolicy,
			name: name,
			imageUrlSquare: imageUrlSquare,
			description: description,
			permissionPolicySet: PermissionPolicySet.toFfiPermissionPolicySet(
				permissionPolicySet),
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	private func newGroupInternalWithInboxIds(
		with inboxIds: [String],
		permissions: FfiGroupPermissionsOptions = .default,
		name: String = "",
		imageUrlSquare: String = "",
		description: String = "",
		permissionPolicySet: FfiPermissionPolicySet? = nil,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		if inboxIds.contains(where: {
			$0.lowercased() == client.inboxID.lowercased()
		}) {
			throw ConversationError.memberCannotBeSelf
		}
		let group = try await ffiConversations.createGroupWithInboxIds(
			inboxIds: inboxIds,
			opts: FfiCreateGroupOptions(
				permissions: permissions,
				groupName: name,
				groupImageUrlSquare: imageUrlSquare,
				groupDescription: description,
				customPermissionPolicySet: permissionPolicySet,
				messageDisappearingSettings: FfiMessageDisappearingSettings(
					fromNs: disappearingMessageSettings?
						.disappearStartingAtNs ?? 0,
					inNs: disappearingMessageSettings?
						.retentionDurationInNs ?? 0
				)
			)
		).groupFromFFI(client: client)
		return group
	}

	public func streamAllMessages(type: ConversationType = .all)
		-> AsyncThrowingStream<Message, Error>
	{
		AsyncThrowingStream { continuation in
			let ffiStreamActor = FfiStreamActor()

			let messageCallback = MessageCallback {
				message in
				guard !Task.isCancelled else {
					continuation.finish()
					Task {
						await ffiStreamActor.endStream()
					}
					return
				}
				if let message = Message.create(ffiMessage: message) {
					continuation.yield(message)
				}
			}

			let task = Task {
				let stream: FfiStreamCloser
				switch type {
				case .groups:
					stream = await ffiConversations.streamAllGroupMessages(
						messageCallback: messageCallback
					)
				case .dms:
					stream = await ffiConversations.streamAllDmMessages(
						messageCallback: messageCallback
					)
				case .all:
					stream = await ffiConversations.streamAllMessages(
						messageCallback: messageCallback
					)
				}
				await ffiStreamActor.setFfiStream(stream)
			}

			continuation.onTermination = { _ in
				task.cancel()
				Task {
					await ffiStreamActor.endStream()
				}
			}
		}
	}

	public func fromWelcome(envelopeBytes: Data) async throws
		-> Conversation?
	{
		let conversation =
			try await ffiConversations
			.processStreamedWelcomeMessage(envelopeBytes: envelopeBytes)
		return try await conversation.toConversation(client: client)
	}

	public func getHmacKeys() throws
		-> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse
	{
		var hmacKeysResponse =
			Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse()
		let conversations = try ffiConversations.getHmacKeys()
		for convo in conversations {
			var hmacKeys =
				Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse.HmacKeys()
			for key in convo.value {
				var hmacKeyData =
					Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse
					.HmacKeyData()
				hmacKeyData.hmacKey = key.key
				hmacKeyData.thirtyDayPeriodsSinceEpoch = Int32(key.epoch)
				hmacKeys.values.append(hmacKeyData)

			}
			hmacKeysResponse.hmacKeys[
				Topic.groupMessage(convo.key.toHex).description] = hmacKeys
		}

		return hmacKeysResponse
	}

}
