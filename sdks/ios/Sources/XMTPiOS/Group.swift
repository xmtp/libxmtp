import Foundation

final class MessageCallback: FfiMessageCallback {
	func onClose() {
		onCloseCallback()
	}

	func onError(error: FfiError) {
		print("Error MessageCallback \(error)")
	}

	let onCloseCallback: () -> Void
	let callback: (FfiMessage) -> Void

	init(
		callback: @escaping (FfiMessage) -> Void,
		onClose: @escaping () -> Void
	) {
		self.callback = callback
		onCloseCallback = onClose
	}

	func onMessage(message: FfiMessage) {
		callback(message)
	}
}

final class StreamHolder {
	var stream: FfiStreamCloser?
}

public enum GroupMembershipState {
	case allowed, rejected, pending, restored, pendingRemove
}

public struct Group: Identifiable, Equatable, Hashable {
	var ffiGroup: FfiConversation
	var ffiLastMessage: FfiMessage?
	var ffiCommitLogForkStatus: Bool?
	var client: Client
	let streamHolder = StreamHolder()

	public var id: String {
		ffiGroup.id().toHex
	}

	public var topic: String {
		Topic.groupMessage(id).description
	}

	public var disappearingMessageSettings: DisappearingMessageSettings? {
		try? {
			guard try isDisappearingMessagesEnabled() else { return nil }
			return try ffiGroup.conversationMessageDisappearingSettings()
				.map { DisappearingMessageSettings.createFromFfi($0) }
		}()
	}

	public func isDisappearingMessagesEnabled() throws -> Bool {
		try ffiGroup.isConversationMessageDisappearingEnabled()
	}

	func metadata() async throws -> FfiConversationMetadata {
		try await ffiGroup.groupMetadata()
	}

	func permissions() throws -> FfiGroupPermissions {
		try ffiGroup.groupPermissions()
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
		try ffiGroup.isActive()
	}

	public func isCreator() async throws -> Bool {
		try await metadata().creatorInboxId() == client.inboxID
	}

	public func isAdmin(inboxId: InboxId) throws -> Bool {
		try ffiGroup.isAdmin(inboxId: inboxId)
	}

	public func isSuperAdmin(inboxId: InboxId) throws -> Bool {
		try ffiGroup.isSuperAdmin(inboxId: inboxId)
	}

	public func addAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.addAdmin(inboxId: inboxId)
	}

	public func removeAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.removeAdmin(inboxId: inboxId)
	}

	public func addSuperAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.addSuperAdmin(inboxId: inboxId)
	}

	public func removeSuperAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.removeSuperAdmin(inboxId: inboxId)
	}

	public func listAdmins() throws -> [InboxId] {
		try ffiGroup.adminList()
	}

	public func listSuperAdmins() throws -> [InboxId] {
		try ffiGroup.superAdminList()
	}

	public func permissionPolicySet() throws -> PermissionPolicySet {
		try PermissionPolicySet.fromFfiPermissionPolicySet(
			permissions().policySet()
		)
	}

	public func creatorInboxId() async throws -> InboxId {
		try await metadata().creatorInboxId()
	}

	public func addedByInboxId() throws -> InboxId {
		try ffiGroup.addedByInboxId()
	}

	public var members: [Member] {
		get async throws {
			try await ffiGroup.listMembers().map { ffiGroupMember in
				Member(ffiGroupMember: ffiGroupMember)
			}
		}
	}

	public var membershipState: GroupMembershipState {
		get throws {
			try ffiGroup.membershipState().fromFFI
		}
	}

	public var peerInboxIds: [InboxId] {
		get async throws {
			var ids = try await members.map(\.inboxId)
			if let index = ids.firstIndex(of: client.inboxID) {
				ids.remove(at: index)
			}
			return ids
		}
	}

	public var createdAt: Date {
		Date(millisecondsSinceEpoch: ffiGroup.createdAtNs())
	}

	public var createdAtNs: Int64 {
		ffiGroup.createdAtNs()
	}

	public var lastActivityAtNs: Int64 {
		ffiLastMessage?.sentAtNs ?? createdAtNs
	}

	public func addMembers(inboxIds: [InboxId]) async throws
		-> GroupMembershipResult
	{
		try validateInboxIds(inboxIds)
		let result = try await ffiGroup.addMembers(inboxIds: inboxIds)
		return GroupMembershipResult(ffiGroupMembershipResult: result)
	}

	public func removeMembers(inboxIds: [InboxId]) async throws {
		try validateInboxIds(inboxIds)
		try await ffiGroup.removeMembers(inboxIds: inboxIds)
	}

	public func addMembersByIdentity(identities: [PublicIdentity]) async throws
		-> GroupMembershipResult
	{
		let result = try await ffiGroup.addMembersByIdentity(
			accountIdentifiers: identities.map(\.ffiPrivate)
		)
		return GroupMembershipResult(ffiGroupMembershipResult: result)
	}

	public func removeMembersByIdentity(identities: [PublicIdentity])
		async throws
	{
		try await ffiGroup.removeMembersByIdentity(
			accountIdentifiers: identities.map(\.ffiPrivate)
		)
	}

	public func name() throws -> String {
		try ffiGroup.groupName()
	}

	public func imageUrl() throws -> String {
		try ffiGroup.groupImageUrlSquare()
	}

	public func description() throws -> String {
		try ffiGroup.groupDescription()
	}

	public func appData() throws -> String {
		try ffiGroup.appData()
	}

	public func updateName(name: String) async throws {
		try await ffiGroup.updateGroupName(groupName: name)
	}

	public func updateImageUrl(imageUrl: String) async throws {
		try await ffiGroup.updateGroupImageUrlSquare(
			groupImageUrlSquare: imageUrl
		)
	}

	public func updateDescription(description: String) async throws {
		try await ffiGroup.updateGroupDescription(
			groupDescription: description
		)
	}

	public func updateAppData(appData: String) async throws {
		try await ffiGroup.updateAppData(appData: appData)
	}

	public func updateAddMemberPermission(newPermissionOption: PermissionOption)
		async throws
	{
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.addMember,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption
			), metadataField: nil
		)
	}

	public func updateRemoveMemberPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.removeMember,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption
			), metadataField: nil
		)
	}

	public func updateAddAdminPermission(newPermissionOption: PermissionOption)
		async throws
	{
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.addAdmin,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption
			), metadataField: nil
		)
	}

	public func updateRemoveAdminPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.removeAdmin,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption
			), metadataField: nil
		)
	}

	public func updateNamePermission(newPermissionOption: PermissionOption)
		async throws
	{
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.updateMetadata,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption
			),
			metadataField: FfiMetadataField.groupName
		)
	}

	public func updateDescriptionPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.updateMetadata,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption
			),
			metadataField: FfiMetadataField.description
		)
	}

	public func updateImageUrlPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.updateMetadata,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption
			),
			metadataField: FfiMetadataField.imageUrlSquare
		)
	}

	public func updateDisappearingMessageSettings(
		_ disappearingMessageSettings: DisappearingMessageSettings?
	) async throws {
		if let settings = disappearingMessageSettings {
			let ffiSettings = FfiMessageDisappearingSettings(
				fromNs: settings.disappearStartingAtNs,
				inNs: settings.retentionDurationInNs
			)
			try await ffiGroup.updateConversationMessageDisappearingSettings(
				settings: ffiSettings
			)
		} else {
			try await clearDisappearingMessageSettings()
		}
	}

	public func clearDisappearingMessageSettings() async throws {
		try await ffiGroup.removeConversationMessageDisappearingSettings()
	}

	/// Returns null if group is not paused, otherwise the min version required to unpause this group
	public func pausedForVersion() throws -> String? {
		try ffiGroup.pausedForVersion()
	}

	public func updateConsentState(state: ConsentState) async throws {
		try ffiGroup.updateConsentState(state: state.toFFI)
	}

	public func consentState() throws -> ConsentState {
		try ffiGroup.consentState().fromFFI
	}

	public func processMessage(messageBytes: Data) async throws
		-> DecodedMessage?
	{
		let messages = try await ffiGroup.processStreamedConversationMessage(
			envelopeBytes: messageBytes
		)
		guard let firstMessage = messages.first else {
			return nil
		}
		return DecodedMessage.create(ffiMessage: firstMessage)
	}

	public func send(content: some Any, options: SendOptions? = nil) async throws
		-> String
	{
		let (encodeContent, visibilityOptions) = try await encodeContent(
			content: content, options: options
		)
		return try await send(encodedContent: encodeContent, visibilityOptions: visibilityOptions)
	}

	public func send(
		encodedContent: EncodedContent, visibilityOptions: MessageVisibilityOptions? = nil
	) async throws -> String {
		do {
			let opts = visibilityOptions?.toFfi() ?? FfiSendMessageOpts(shouldPush: true)
			let messageId = try await ffiGroup.send(
				contentBytes: encodedContent.serializedData(),
				opts: opts
			)
			return messageId.toHex
		} catch {
			throw error
		}
	}

	public func encodeContent<T>(content: T, options: SendOptions?) async throws
		-> (EncodedContent, MessageVisibilityOptions)
	{
		let codec = Client.codecRegistry.find(for: options?.contentType)

		func encode<Codec: ContentCodec>(codec: Codec, content: Any) throws
			-> EncodedContent
		{
			if let content = content as? Codec.T {
				return try codec.encode(content: content)
			} else {
				throw CodecError.invalidContent
			}
		}

		var encoded = try encode(codec: codec, content: content)

		func fallback<Codec: ContentCodec>(codec: Codec, content: Any) throws
			-> String?
		{
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

		func shouldPush<Codec: ContentCodec>(codec: Codec, content: Any) throws
			-> Bool
		{
			if let content = content as? Codec.T {
				return try codec.shouldPush(content: content)
			} else {
				throw CodecError.invalidContent
			}
		}

		let visibilityOptions = try MessageVisibilityOptions(
			shouldPush: shouldPush(codec: codec, content: content)
		)

		return (encoded, visibilityOptions)
	}

	public func prepareMessage(
		encodedContent: EncodedContent,
		visibilityOptions: MessageVisibilityOptions? = nil,
		noSend: Bool = false
	) async throws
		-> String
	{
		let shouldPush = visibilityOptions?.shouldPush ?? true
		let messageId: Data
		if noSend {
			messageId = try ffiGroup.prepareMessage(
				contentBytes: encodedContent.serializedData(),
				shouldPush: shouldPush
			)
		} else {
			let opts = visibilityOptions?.toFfi() ?? FfiSendMessageOpts(shouldPush: true)
			messageId = try ffiGroup.sendOptimistic(
				contentBytes: encodedContent.serializedData(),
				opts: opts
			)
		}
		return messageId.toHex
	}

	public func prepareMessage(content: some Any, options: SendOptions? = nil, noSend: Bool = false)
		async throws -> String
	{
		let (encodeContent, visibilityOptions) = try await encodeContent(
			content: content, options: options
		)
		return try await prepareMessage(
			encodedContent: encodeContent,
			visibilityOptions: visibilityOptions,
			noSend: noSend
		)
	}

	public func publishMessages() async throws {
		try await ffiGroup.publishMessages()
	}

	public func publishMessage(messageId: String) async throws {
		try await ffiGroup.publishStoredMessage(messageId: messageId.hexToData)
	}

	public func endStream() {
		streamHolder.stream?.end()
	}

	public func streamMessages(onClose: (() -> Void)? = nil)
		-> AsyncThrowingStream<DecodedMessage, Error>
	{
		AsyncThrowingStream { continuation in
			let task = Task.detached {
				streamHolder.stream = await ffiGroup.stream(
					messageCallback: MessageCallback { message in
						guard !Task.isCancelled else {
							continuation.finish()
							return
						}
						if let message = DecodedMessage.create(
							ffiMessage: message
						) {
							continuation.yield(message)
						}
					} onClose: {
						onClose?()
						continuation.finish()
					}
				)

				continuation.onTermination = { @Sendable _ in
					streamHolder.stream?.end()
				}
			}

			continuation.onTermination = { @Sendable _ in
				task.cancel()
				streamHolder.stream?.end()
			}
		}
	}

	public func lastMessage() async throws -> DecodedMessage? {
		if let ffiMessage = ffiLastMessage {
			DecodedMessage.create(ffiMessage: ffiMessage)
		} else {
			try await messages(limit: 1).first
		}
	}

	public func commitLogForkStatus() -> CommitLogForkStatus {
		switch ffiCommitLogForkStatus {
		case true: .forked
		case false: .notForked
		default: .unknown
		}
	}

	/// Get the raw list of messages from a conversation.
	///
	/// This method returns all messages in chronological order without additional processing.
	/// Reactions, replies, and other associated metadata are returned as separate messages
	/// and are not linked to their parent messages.
	///
	/// For UI rendering, consider using ``enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	/// instead,
	/// which provides messages with enriched metadata automatically included.
	///
	/// - SeeAlso: ``enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	public func messages(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil,
			excludeContentTypes: nil,
			excludeSenderInboxIds: nil,
			sortBy: nil,
			insertedAfterNs: nil,
			insertedBeforeNs: nil
		)

		if let beforeNs {
			options.sentBeforeNs = beforeNs
		}

		if let afterNs {
			options.sentAfterNs = afterNs
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let status: FfiDeliveryStatus? = switch deliveryStatus {
		case .published:
			FfiDeliveryStatus.published
		case .unpublished:
			FfiDeliveryStatus.unpublished
		case .failed:
			FfiDeliveryStatus.failed
		default:
			nil
		}

		options.deliveryStatus = status

		let direction: FfiDirection? = switch direction {
		case .ascending:
			FfiDirection.ascending
		default:
			FfiDirection.descending
		}

		options.direction = direction
		options.excludeContentTypes = excludeContentTypes
		options.excludeSenderInboxIds = excludeSenderInboxIds
		options.sortBy = sortBy?.toFfi()
		options.insertedAfterNs = insertedAfterNs
		options.insertedBeforeNs = insertedBeforeNs

		return try await ffiGroup.findMessages(opts: options).compactMap {
			ffiMessage in
			DecodedMessage.create(ffiMessage: ffiMessage)
		}
	}

	public func messagesWithReactions(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil,
			excludeContentTypes: nil,
			excludeSenderInboxIds: nil,
			sortBy: nil,
			insertedAfterNs: nil,
			insertedBeforeNs: nil
		)

		if let beforeNs {
			options.sentBeforeNs = beforeNs
		}

		if let afterNs {
			options.sentAfterNs = afterNs
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let status: FfiDeliveryStatus? = switch deliveryStatus {
		case .published:
			FfiDeliveryStatus.published
		case .unpublished:
			FfiDeliveryStatus.unpublished
		case .failed:
			FfiDeliveryStatus.failed
		default:
			nil
		}

		options.deliveryStatus = status

		let direction: FfiDirection? = switch direction {
		case .ascending:
			FfiDirection.ascending
		default:
			FfiDirection.descending
		}

		options.direction = direction
		options.excludeContentTypes = excludeContentTypes
		options.excludeSenderInboxIds = excludeSenderInboxIds
		options.sortBy = sortBy?.toFfi()
		options.insertedAfterNs = insertedAfterNs
		options.insertedBeforeNs = insertedBeforeNs

		return try ffiGroup.findMessagesWithReactions(opts: options)
			.compactMap {
				ffiMessageWithReactions in
				DecodedMessage.create(
					ffiMessage: ffiMessageWithReactions
				)
			}
	}

	/// Get messages with enriched metadata automatically included.
	///
	/// This method retrieves messages with reactions, replies, and other associated data
	/// "baked in" to each message, eliminating the need for separate queries to fetch
	/// this information.
	///
	/// **Recommended for UI rendering.** This method provides better performance and
	/// simpler code compared to ``messages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	/// when displaying conversations.
	///
	/// When handling content types, use the generic `content<T>()` method with the
	/// appropriate type for reactions and replies.
	///
	/// - Returns: Array of `DecodedMessageV2` with enriched metadata.
	/// - SeeAlso: ``messages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)``
	public func enrichedMessages(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		sortBy: MessageSortBy? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) async throws -> [DecodedMessageV2] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil,
			excludeContentTypes: nil,
			excludeSenderInboxIds: nil,
			sortBy: nil,
			insertedAfterNs: nil,
			insertedBeforeNs: nil
		)

		if let beforeNs {
			options.sentBeforeNs = beforeNs
		}

		if let afterNs {
			options.sentAfterNs = afterNs
		}

		if let limit {
			options.limit = Int64(limit)
		}

		let status: FfiDeliveryStatus? = switch deliveryStatus {
		case .published:
			FfiDeliveryStatus.published
		case .unpublished:
			FfiDeliveryStatus.unpublished
		case .failed:
			FfiDeliveryStatus.failed
		default:
			nil
		}

		options.deliveryStatus = status

		let direction: FfiDirection? = switch direction {
		case .ascending:
			FfiDirection.ascending
		default:
			FfiDirection.descending
		}

		options.direction = direction
		options.excludeContentTypes = excludeContentTypes
		options.excludeSenderInboxIds = excludeSenderInboxIds
		options.sortBy = sortBy?.toFfi()
		options.insertedAfterNs = insertedAfterNs
		options.insertedBeforeNs = insertedBeforeNs

		return try await ffiGroup.findEnrichedMessages(opts: options).compactMap {
			ffiDecodedMessage in
			DecodedMessageV2(ffiMessage: ffiDecodedMessage)
		}
	}

	public func countMessages(
		beforeNs: Int64? = nil, afterNs: Int64? = nil, deliveryStatus: MessageDeliveryStatus = .all,
		excludeContentTypes: [StandardContentType]? = nil,
		excludeSenderInboxIds: [String]? = nil,
		insertedAfterNs: Int64? = nil,
		insertedBeforeNs: Int64? = nil
	) throws -> Int64 {
		try ffiGroup.countMessages(
			opts: FfiListMessagesOptions(
				sentBeforeNs: beforeNs,
				sentAfterNs: afterNs,
				limit: nil,
				deliveryStatus: deliveryStatus.toFfi(),
				direction: .descending,
				contentTypes: nil,
				excludeContentTypes: excludeContentTypes,
				excludeSenderInboxIds: excludeSenderInboxIds,
				sortBy: nil,
				insertedAfterNs: insertedAfterNs,
				insertedBeforeNs: insertedBeforeNs
			)
		)
	}

	public func getHmacKeys() throws
		-> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse
	{
		var hmacKeysResponse =
			Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse()
		let conversations: [Data: [FfiHmacKey]] = try ffiGroup.getHmacKeys()
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
				Topic.groupMessage(convo.key.toHex).description
			] = hmacKeys
		}

		return hmacKeysResponse
	}

	public func getPushTopics() throws -> [String] {
		[topic]
	}

	public func getDebugInformation() async throws -> ConversationDebugInfo {
		try await ConversationDebugInfo(
			ffiConversationDebugInfo: ffiGroup.conversationDebugInfo()
		)
	}

	public func getLastReadTimes() throws -> [String: Int64] {
		try ffiGroup.getLastReadTimes()
	}

	public func leaveGroup() async throws {
		try await ffiGroup.leaveGroup()
	}

	/// Delete a message by its ID.
	/// - Parameter messageId: The hex-encoded message ID to delete.
	/// - Returns: The hex-encoded ID of the deletion message.
	/// - Throws: An error if the deletion fails (e.g., unauthorized deletion).
	public func deleteMessage(messageId: String) async throws -> String {
		try await ffiGroup.deleteMessage(messageId: messageId.hexToData).toHex
	}
}
