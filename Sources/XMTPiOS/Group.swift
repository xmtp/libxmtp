import Foundation
import LibXMTP

final class MessageCallback: FfiMessageCallback {
	func onError(error: LibXMTP.FfiSubscribeError) {
		print("Error MessageCallback \(error)")
	}

	let callback: (LibXMTP.FfiMessage) -> Void

	init(_ callback: @escaping (LibXMTP.FfiMessage) -> Void) {
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
	var ffiGroup: FfiConversation
	var ffiLastMessage: FfiMessage? = nil
	var client: Client
	let streamHolder = StreamHolder()

	public var id: String {
		ffiGroup.id().toHex
	}

	public var topic: String {
		Topic.groupMessage(id).description
	}

	public var disappearingMessageSettings: DisappearingMessageSettings? {
		return try? {
			guard try isDisappearingMessagesEnabled() else { return nil }
			return try ffiGroup.conversationMessageDisappearingSettings()
				.map { DisappearingMessageSettings.createFromFfi($0) }
		}()
	}

	public func isDisappearingMessagesEnabled() throws -> Bool {
		return try ffiGroup.isConversationMessageDisappearingEnabled()
	}

	func metadata() async throws -> FfiConversationMetadata {
		return try await ffiGroup.groupMetadata()
	}

	func permissions() throws -> FfiGroupPermissions {
		return try ffiGroup.groupPermissions()
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

	public func isCreator() async throws -> Bool {
		return try await metadata().creatorInboxId() == client.inboxID
	}

	public func isAdmin(inboxId: InboxId) throws -> Bool {
		return try ffiGroup.isAdmin(inboxId: inboxId)
	}

	public func isSuperAdmin(inboxId: InboxId) throws -> Bool {
		return try ffiGroup.isSuperAdmin(inboxId: inboxId)
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
		return PermissionPolicySet.fromFfiPermissionPolicySet(
			try permissions().policySet())
	}

	public func creatorInboxId() async throws -> InboxId {
		return try await metadata().creatorInboxId()
	}

	public func addedByInboxId() throws -> InboxId {
		return try ffiGroup.addedByInboxId()
	}

	public var members: [Member] {
		get async throws {
			return try await ffiGroup.listMembers().map { ffiGroupMember in
				Member(ffiGroupMember: ffiGroupMember)
			}
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

	public func addMembers(inboxIds: [InboxId]) async throws -> GroupMembershipResult {
		try validateInboxIds(inboxIds)
		let result = try await ffiGroup.addMembersByInboxId(inboxIds: inboxIds)
		return GroupMembershipResult(ffiGroupMembershipResult: result)
	}

	public func removeMembers(inboxIds: [InboxId]) async throws {
		try validateInboxIds(inboxIds)
		try await ffiGroup.removeMembersByInboxId(inboxIds: inboxIds)
	}

	public func addMembersByIdentity(identities: [PublicIdentity]) async throws -> GroupMembershipResult
	{
		let result = try await ffiGroup.addMembers(
			accountIdentifiers: identities.map { $0.ffiPrivate })
		return GroupMembershipResult(ffiGroupMembershipResult: result)
	}

	public func removeMembersByIdentity(identities: [PublicIdentity])
		async throws
	{
		try await ffiGroup.removeMembers(
			accountIdentifiers: identities.map { $0.ffiPrivate })
	}

	public func name() throws -> String {
		return try ffiGroup.groupName()
	}

	public func imageUrl() throws -> String {
		return try ffiGroup.groupImageUrlSquare()
	}

	public func description() throws -> String {
		return try ffiGroup.groupDescription()
	}

	public func updateName(name: String) async throws {
		try await ffiGroup.updateGroupName(groupName: name)
	}

	public func updateImageUrl(imageUrl: String) async throws {
		try await ffiGroup.updateGroupImageUrlSquare(
			groupImageUrlSquare: imageUrl)
	}

	public func updateDescription(description: String) async throws {
		try await ffiGroup.updateGroupDescription(
			groupDescription: description)
	}

	public func updateAddMemberPermission(newPermissionOption: PermissionOption)
		async throws
	{
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.addMember,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption), metadataField: nil)
	}

	public func updateRemoveMemberPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.removeMember,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption), metadataField: nil)
	}

	public func updateAddAdminPermission(newPermissionOption: PermissionOption)
		async throws
	{
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.addAdmin,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption), metadataField: nil)
	}

	public func updateRemoveAdminPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.removeAdmin,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption), metadataField: nil)
	}

	public func updateNamePermission(newPermissionOption: PermissionOption)
		async throws
	{
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.updateMetadata,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption),
			metadataField: FfiMetadataField.groupName)
	}

	public func updateDescriptionPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.updateMetadata,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption),
			metadataField: FfiMetadataField.description)
	}

	public func updateImageUrlPermission(
		newPermissionOption: PermissionOption
	) async throws {
		try await ffiGroup.updatePermissionPolicy(
			permissionUpdateType: FfiPermissionUpdateType.updateMetadata,
			permissionPolicyOption: PermissionOption.toFfiPermissionPolicy(
				option: newPermissionOption),
			metadataField: FfiMetadataField.imageUrlSquare)
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
				settings: ffiSettings)
		} else {
			try await clearDisappearingMessageSettings()
		}
	}

    public func clearDisappearingMessageSettings() async throws {
        try await ffiGroup.removeConversationMessageDisappearingSettings()
    }
    
    // Returns null if group is not paused, otherwise the min version required to unpause this group
    public func pausedForVersion() throws -> String? {
        return try ffiGroup.pausedForVersion()
    }
    
    public func updateConsentState(state: ConsentState) async throws {
		try ffiGroup.updateConsentState(state: state.toFFI)
	}

	public func consentState() throws -> ConsentState {
		return try ffiGroup.consentState().fromFFI
	}

	public func processMessage(messageBytes: Data) async throws -> DecodedMessage? {
		let message = try await ffiGroup.processStreamedConversationMessage(
			envelopeBytes: messageBytes)
		return DecodedMessage.create(ffiMessage: message)
	}

	public func send<T>(content: T, options: SendOptions? = nil) async throws
		-> String
	{
		let encodeContent = try await encodeContent(
			content: content, options: options)
		return try await send(encodedContent: encodeContent)
	}

	public func send(encodedContent: EncodedContent) async throws -> String {
		do {
			let messageId = try await ffiGroup.send(
				contentBytes: encodedContent.serializedData())
			return messageId.toHex
		} catch {
			throw error
		}
	}

	public func encodeContent<T>(content: T, options: SendOptions?) async throws
		-> EncodedContent
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

		return encoded
	}

	public func prepareMessage(encodedContent: EncodedContent) async throws
		-> String
	{
		let messageId = try ffiGroup.sendOptimistic(
			contentBytes: encodedContent.serializedData())
		return messageId.toHex
	}

	public func prepareMessage<T>(content: T, options: SendOptions? = nil)
		async throws -> String
	{
		let encodeContent = try await encodeContent(
			content: content, options: options)
		return try ffiGroup.sendOptimistic(
			contentBytes: try encodeContent.serializedData()
		).toHex
	}

	public func publishMessages() async throws {
		try await ffiGroup.publishMessages()
	}

	public func endStream() {
		self.streamHolder.stream?.end()
	}

	public func streamMessages() -> AsyncThrowingStream<DecodedMessage, Error> {
		AsyncThrowingStream { continuation in
			let task = Task.detached {
				self.streamHolder.stream = await self.ffiGroup.stream(
					messageCallback: MessageCallback {
						message in
						guard !Task.isCancelled else {
							continuation.finish()
							return
						}
						if let message = DecodedMessage.create(ffiMessage: message) {
							continuation.yield(message)
						}
					}
				)

				continuation.onTermination = { @Sendable reason in
					self.streamHolder.stream?.end()
				}
			}

			continuation.onTermination = { @Sendable reason in
				task.cancel()
				self.streamHolder.stream?.end()
			}
		}
	}

	public func lastMessage() async throws -> DecodedMessage? {
		if let ffiMessage = ffiLastMessage {
			return DecodedMessage.create(ffiMessage: ffiMessage)
		} else {
			return try await messages(limit: 1).first
		}
	}

	public func messages(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil
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

		let status: FfiDeliveryStatus? = {
			switch deliveryStatus {
			case .published:
				return FfiDeliveryStatus.published
			case .unpublished:
				return FfiDeliveryStatus.unpublished
			case .failed:
				return FfiDeliveryStatus.failed
			default:
				return nil
			}
		}()

		options.deliveryStatus = status

		let direction: FfiDirection? = {
			switch direction {
			case .ascending:
				return FfiDirection.ascending
			default:
				return FfiDirection.descending
			}
		}()

		options.direction = direction

		return try await ffiGroup.findMessages(opts: options).compactMap {
			ffiMessage in
			return DecodedMessage.create(ffiMessage: ffiMessage)
		}
	}

	public func messagesWithReactions(
		beforeNs: Int64? = nil,
		afterNs: Int64? = nil,
		limit: Int? = nil,
		direction: SortDirection? = .descending,
		deliveryStatus: MessageDeliveryStatus = .all
	) async throws -> [DecodedMessage] {
		var options = FfiListMessagesOptions(
			sentBeforeNs: nil,
			sentAfterNs: nil,
			limit: nil,
			deliveryStatus: nil,
			direction: nil,
			contentTypes: nil
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

		let status: FfiDeliveryStatus? = {
			switch deliveryStatus {
			case .published:
				return FfiDeliveryStatus.published
			case .unpublished:
				return FfiDeliveryStatus.unpublished
			case .failed:
				return FfiDeliveryStatus.failed
			default:
				return nil
			}
		}()

		options.deliveryStatus = status

		let direction: FfiDirection? = {
			switch direction {
			case .ascending:
				return FfiDirection.ascending
			default:
				return FfiDirection.descending
			}
		}()

		options.direction = direction

		return try await ffiGroup.findMessagesWithReactions(opts: options)
			.compactMap {
				ffiMessageWithReactions in
				return DecodedMessage.create(ffiMessage: ffiMessageWithReactions)
			}
	}
    
    public func getHmacKeys() throws
    -> Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse {
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
				Topic.groupMessage(convo.key.toHex).description] = hmacKeys
		}

		return hmacKeysResponse
    }
    
    public func getPushTopics() throws -> [String] {
        return [topic]
    }
    
    public func getDebugInformation() async throws -> ConversationDebugInfo {
        return ConversationDebugInfo(ffiConversationDebugInfo: try await ffiGroup.conversationDebugInfo())
    }
}
