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

public enum ConversationFilterType {
	case all, groups, dms
}

public enum ConversationsOrderBy {
    case createdAt, lastActivity
    
    fileprivate var ffiOrderBy: FfiGroupQueryOrderBy {
        switch self {
        case .createdAt: return .createdAt
        case .lastActivity: return .lastActivity
        }
    }
}

final class ConversationStreamCallback: FfiConversationCallback {
	let onCloseCallback: () -> Void
	let callback: (FfiConversation) -> Void

	init(
		callback: @escaping (FfiConversation) -> Void,
		onClose: @escaping () -> Void
	) {
		self.callback = callback
		self.onCloseCallback = onClose
	}

	func onClose() {
		self.onCloseCallback()
	}

	func onError(error: LibXMTP.FfiSubscribeError) {
		print("Error ConversationStreamCallback \(error)")
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
public class Conversations {
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

	/// Helper function to convert DisappearingMessageSettings to FfiMessageDisappearingSettings
	/// Returns nil if the input is nil, making it explicit that nil will be passed to FFI
	private func toFfiDisappearingMessageSettings(_ settings: DisappearingMessageSettings?) -> FfiMessageDisappearingSettings? {
		guard let settings = settings else { return nil }
		return FfiMessageDisappearingSettings(
			fromNs: settings.disappearStartingAtNs,
			inNs: settings.retentionDurationInNs
		)
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

	public func findDmByInboxId(inboxId: InboxId) throws -> Dm? {
		do {
			let conversation = try ffiClient.dmConversation(
				targetInboxId: inboxId)
			return Dm(
				ffiConversation: conversation, client: client)
		} catch {
			return nil
		}
	}

	public func findDmByIdentity(publicIdentity: PublicIdentity) async throws
		-> Dm?
	{
		guard
			let inboxId = try await client.inboxIdFromIdentity(
				identity: publicIdentity)
		else {
			throw ClientError.creationError("No inboxId present")
		}
		return try findDmByInboxId(inboxId: inboxId)
	}

	public func findMessage(messageId: String) throws -> DecodedMessage? {
		do {
			return DecodedMessage.create(
				ffiMessage: try ffiClient.message(
					messageId: messageId.hexToData))
		} catch {
			return nil
		}
	}

	public func findEnrichedMessage(messageId: String) throws -> DecodedMessageV2? {
		do {
			return DecodedMessageV2.create(ffiMessage: try ffiClient.messageV2(messageId: messageId.hexToData))
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
            createdAfterNs: Int64? = nil,
            createdBeforeNs: Int64? = nil,
            lastActivityAfterNs: Int64? = nil,
            lastActivityBeforeNs: Int64? = nil,
            limit: Int? = nil,
            consentStates: [ConsentState]? = nil,
            orderBy: ConversationsOrderBy = ConversationsOrderBy.lastActivity
        ) throws -> [Group] {
            var options = FfiListConversationsOptions(
                createdAfterNs: createdAfterNs,
                createdBeforeNs: createdBeforeNs,
                lastActivityBeforeNs: lastActivityBeforeNs,
                lastActivityAfterNs: lastActivityAfterNs,
                orderBy: orderBy.ffiOrderBy,
                limit: nil,
                consentStates: consentStates?.toFFI,
                includeDuplicateDms: false
            )

            if let limit {
                options.limit = Int64(limit)
            }
            let conversations = try ffiConversations.listGroups(
                opts: options
            )

            return conversations.map {
                $0.groupFromFFI(client: client)
            }
        }

        public func listDms(
            createdAfterNs: Int64? = nil,
            createdBeforeNs: Int64? = nil,
            lastActivityBeforeNs: Int64? = nil,
            lastActivityAfterNs: Int64? = nil,
            limit: Int? = nil,
            consentStates: [ConsentState]? = nil,
            orderBy: ConversationsOrderBy = ConversationsOrderBy.lastActivity
        ) throws -> [Dm] {
            var options = FfiListConversationsOptions(
                createdAfterNs: createdAfterNs,
                createdBeforeNs: createdBeforeNs,
                lastActivityBeforeNs: lastActivityBeforeNs,
                lastActivityAfterNs: lastActivityAfterNs,
                orderBy: orderBy.ffiOrderBy,
                limit: nil,
                consentStates: consentStates?.toFFI,
                includeDuplicateDms: false  
            )

            if let limit {
                options.limit = Int64(limit)
            }

            let conversations = try ffiConversations.listDms(
                opts: options
            )

            return conversations.map {
                $0.dmFromFFI(client: client)
            }
        }

        public func list(
            createdAfterNs: Int64? = nil,
            createdBeforeNs: Int64? = nil,
            lastActivityBeforeNs: Int64? = nil,
            lastActivityAfterNs: Int64? = nil,
            limit: Int? = nil,
            consentStates: [ConsentState]? = nil,
            orderBy: ConversationsOrderBy = ConversationsOrderBy.lastActivity
        ) async throws -> [Conversation] {
            var options = FfiListConversationsOptions(
                createdAfterNs: createdAfterNs,
                createdBeforeNs: createdBeforeNs,
                lastActivityBeforeNs: lastActivityBeforeNs,
                lastActivityAfterNs: lastActivityAfterNs,
                orderBy: orderBy.ffiOrderBy,
                limit: nil,
                consentStates: consentStates?.toFFI,
                includeDuplicateDms: false
            )

            if let limit {
                options.limit = Int64(limit)
            }
            let ffiConversations = try ffiConversations.list(
                opts: options
            )

            var conversations: [Conversation] = []
            for conversation in ffiConversations {
                let conversation = try await conversation.toConversation(
                    client: client
                )
                conversations.append(conversation)
            }
            return conversations
        }

	public func stream(
		type: ConversationFilterType = .all, onClose: (() -> Void)? = nil
	) -> AsyncThrowingStream<
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
			} onClose: {
				onClose?()
				continuation.finish()
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

	public func newConversationWithIdentity(
		with peerIdentity: PublicIdentity,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Conversation {
		let dm = try await findOrCreateDmWithIdentity(
			with: peerIdentity,
			disappearingMessageSettings: disappearingMessageSettings)
		return Conversation.dm(dm)
	}

	public func findOrCreateDmWithIdentity(
		with peerIdentity: PublicIdentity,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Dm {
		if try await client.inboxState(refreshFromNetwork: false).identities
			.map({ $0.identifier }).contains(peerIdentity.identifier)
		{
			throw ConversationError.memberCannotBeSelf
		}

		let dm =
			try await ffiConversations
			.findOrCreateDm(
				targetIdentity: peerIdentity.ffiPrivate,
				opts: FfiCreateDmOptions(
					messageDisappearingSettings: toFfiDisappearingMessageSettings(disappearingMessageSettings)
					))

		return dm.dmFromFFI(client: client)
	}

	public func newConversation(
		with peerInboxId: InboxId,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Conversation {
		let dm = try await findOrCreateDm(
			with: peerInboxId,
			disappearingMessageSettings: disappearingMessageSettings)
		return Conversation.dm(dm)
	}

	public func findOrCreateDm(
		with peerInboxId: InboxId,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	)
		async throws -> Dm
	{
		if peerInboxId == client.inboxID {
			throw ConversationError.memberCannotBeSelf
		}
		try validateInboxId(peerInboxId)
		let dm =
			try await ffiConversations
			.findOrCreateDmByInboxId(
				inboxId: peerInboxId,
				opts: FfiCreateDmOptions(
					messageDisappearingSettings: toFfiDisappearingMessageSettings(disappearingMessageSettings)
					))
		return dm.dmFromFFI(client: client)

	}

	public func newGroupWithIdentities(
		with identities: [PublicIdentity],
		permissions: GroupPermissionPreconfiguration = .allMembers,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternalWithIdentities(
			with: identities,
			permissions:
				GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
					option: permissions),
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: nil,
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	public func newGroupCustomPermissionsWithIdentities(
		with identities: [PublicIdentity],
		permissionPolicySet: PermissionPolicySet,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternalWithIdentities(
			with: identities,
			permissions: FfiGroupPermissionsOptions.customPolicy,
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: PermissionPolicySet.toFfiPermissionPolicySet(
				permissionPolicySet),
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	private func newGroupInternalWithIdentities(
		with identities: [PublicIdentity],
		permissions: FfiGroupPermissionsOptions = .default,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		permissionPolicySet: FfiPermissionPolicySet? = nil,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		let group = try await ffiConversations.createGroup(
			accountIdentities: identities.map { $0.ffiPrivate },
			opts: FfiCreateGroupOptions(
				permissions: permissions,
				groupName: name,
				groupImageUrlSquare: imageUrl,
				groupDescription: description,
				customPermissionPolicySet: permissionPolicySet,
				messageDisappearingSettings: toFfiDisappearingMessageSettings(disappearingMessageSettings)
			)
		).groupFromFFI(client: client)
		return group
	}

	public func newGroup(
		with inboxIds: [InboxId],
		permissions: GroupPermissionPreconfiguration = .allMembers,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternal(
			with: inboxIds,
			permissions:
				GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
					option: permissions),
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: nil,
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	public func newGroupCustomPermissions(
		with inboxIds: [InboxId],
		permissionPolicySet: PermissionPolicySet,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		return try await newGroupInternal(
			with: inboxIds,
			permissions: FfiGroupPermissionsOptions.customPolicy,
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: PermissionPolicySet.toFfiPermissionPolicySet(
				permissionPolicySet),
			disappearingMessageSettings: disappearingMessageSettings
		)
	}

	private func newGroupInternal(
		with inboxIds: [InboxId],
		permissions: FfiGroupPermissionsOptions = .default,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		permissionPolicySet: FfiPermissionPolicySet? = nil,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Group {
		try validateInboxIds(inboxIds)
		let group = try await ffiConversations.createGroupWithInboxIds(
			inboxIds: inboxIds,
			opts: FfiCreateGroupOptions(
				permissions: permissions,
				groupName: name,
				groupImageUrlSquare: imageUrl,
				groupDescription: description,
				customPermissionPolicySet: permissionPolicySet,
				messageDisappearingSettings: toFfiDisappearingMessageSettings(disappearingMessageSettings)
			)
		).groupFromFFI(client: client)
		return group
	}

	public func newGroupOptimistic(
		permissions: GroupPermissionPreconfiguration = .allMembers,
		groupName: String = "",
		groupImageUrlSquare: String = "",
		groupDescription: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) throws -> Group {
		let ffiOpts = FfiCreateGroupOptions(
			permissions:
				GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
					option: permissions),
			groupName: groupName,
			groupImageUrlSquare: groupImageUrlSquare,
			groupDescription: groupDescription,
			customPermissionPolicySet: nil,
			messageDisappearingSettings: toFfiDisappearingMessageSettings(disappearingMessageSettings)
		)

		let ffiGroup = try ffiConversations.createGroupOptimistic(opts: ffiOpts)
		return Group(ffiGroup: ffiGroup, client: client)
	}

	public func streamAllMessages(
		type: ConversationFilterType = .all,
		consentStates: [ConsentState]? = nil,
		onClose: (() -> Void)? = nil
	)
		-> AsyncThrowingStream<DecodedMessage, Error>
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
				if let message = DecodedMessage.create(ffiMessage: message) {
					continuation.yield(message)
				}
			} onClose: {
				onClose?()
				continuation.finish()
			}

			let task = Task {
				let stream: FfiStreamCloser
				switch type {
				case .groups:
					stream = await ffiConversations.streamAllGroupMessages(
						messageCallback: messageCallback,
						consentStates: consentStates?.toFFI
					)
				case .dms:
					stream = await ffiConversations.streamAllDmMessages(
						messageCallback: messageCallback,
						consentStates: consentStates?.toFFI
					)
				case .all:
					stream = await ffiConversations.streamAllMessages(
						messageCallback: messageCallback,
						consentStates: consentStates?.toFFI
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
		let conversations: [Data: [FfiHmacKey]] =
			try ffiConversations.getHmacKeys()
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

	public func allPushTopics() async throws -> [String] {
		let options = FfiListConversationsOptions(
			createdAfterNs: nil,
			createdBeforeNs: nil,
            lastActivityBeforeNs: nil,
            lastActivityAfterNs: nil,
            orderBy: nil,
			limit: nil,
			consentStates: nil,
			includeDuplicateDms: true
		)

		let conversations = try ffiConversations.list(opts: options)
		return conversations.map {
			Topic.groupMessage($0.conversation().id().toHex).description
		}
	}

}
