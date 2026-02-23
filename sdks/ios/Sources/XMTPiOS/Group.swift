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

/// The membership state of the current client within a group conversation.
public enum GroupMembershipState {
	/// The client is an active member of the group.
	case allowed
	/// The client's membership request was rejected.
	case rejected
	/// The client has a pending membership request.
	case pending
	/// The client's membership was previously removed and has been restored.
	case restored
	/// The client has a pending removal from the group.
	case pendingRemove
}

/// A multi-party group conversation on the XMTP network.
///
/// Groups are MLS-based conversations that support multiple participants with
/// role-based access control (admin and super admin roles). They provide
/// features such as metadata (name, image, description), disappearing messages,
/// and configurable permission policies.
///
/// Call ``sync()`` to fetch the latest state from the network before reading
/// group properties that may have changed remotely.
public struct Group: Identifiable, Equatable, Hashable {
	var ffiGroup: FfiConversation
	var ffiLastMessage: FfiMessage?
	var ffiCommitLogForkStatus: Bool?
	var client: Client
	let streamHolder = StreamHolder()

	/// The hex-encoded unique identifier for this group.
	public var id: String {
		ffiGroup.id().toHex
	}

	/// The MLS topic string for this group, used for push notification subscriptions.
	public var topic: String {
		Topic.groupMessage(id).description
	}

	/// The current disappearing message settings for this group, or `nil` if disappearing messages are disabled.
	public var disappearingMessageSettings: DisappearingMessageSettings? {
		try? {
			guard try isDisappearingMessagesEnabled() else { return nil }
			return try ffiGroup.conversationMessageDisappearingSettings()
				.map { DisappearingMessageSettings.createFromFfi($0) }
		}()
	}

	/// Returns whether disappearing messages are enabled for this group.
	/// - Throws: If the underlying FFI call fails.
	public func isDisappearingMessagesEnabled() throws -> Bool {
		try ffiGroup.isConversationMessageDisappearingEnabled()
	}

	func metadata() async throws -> FfiConversationMetadata {
		try await ffiGroup.groupMetadata()
	}

	func permissions() throws -> FfiGroupPermissions {
		try ffiGroup.groupPermissions()
	}

	/// Syncs the group state from the network.
	///
	/// Fetches the latest messages, membership changes, and metadata updates
	/// from the XMTP network. Call this before reading group properties that
	/// may have been updated remotely.
	public func sync() async throws {
		try await ffiGroup.sync()
	}

	public static func == (lhs: Group, rhs: Group) -> Bool {
		lhs.id == rhs.id
	}

	public func hash(into hasher: inout Hasher) {
		id.hash(into: &hasher)
	}

	/// Returns whether the current client is still an active member of the group.
	///
	/// A client becomes inactive when they are removed from the group or leave voluntarily.
	public func isActive() throws -> Bool {
		try ffiGroup.isActive()
	}

	/// Returns whether the current client is the original creator of the group.
	public func isCreator() async throws -> Bool {
		try await metadata().creatorInboxId() == client.inboxID
	}

	/// Returns whether the specified inbox ID has admin privileges in this group.
	/// - Parameter inboxId: The inbox ID to check.
	/// - Returns: `true` if the inbox ID is an admin.
	public func isAdmin(inboxId: InboxId) throws -> Bool {
		try ffiGroup.isAdmin(inboxId: inboxId)
	}

	/// Returns whether the specified inbox ID has super admin privileges in this group.
	/// - Parameter inboxId: The inbox ID to check.
	/// - Returns: `true` if the inbox ID is a super admin.
	public func isSuperAdmin(inboxId: InboxId) throws -> Bool {
		try ffiGroup.isSuperAdmin(inboxId: inboxId)
	}

	/// Promotes a member to admin role.
	///
	/// - Parameter inboxId: The inbox ID of the member to promote.
	/// - Important: Requires super admin permissions.
	public func addAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.addAdmin(inboxId: inboxId)
	}

	/// Demotes an admin to regular member role.
	///
	/// - Parameter inboxId: The inbox ID of the admin to demote.
	/// - Important: Requires super admin permissions.
	public func removeAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.removeAdmin(inboxId: inboxId)
	}

	/// Promotes a member to super admin role.
	///
	/// - Parameter inboxId: The inbox ID of the member to promote.
	/// - Important: Requires super admin permissions.
	public func addSuperAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.addSuperAdmin(inboxId: inboxId)
	}

	/// Demotes a super admin to regular member role.
	///
	/// - Parameter inboxId: The inbox ID of the super admin to demote.
	/// - Important: Requires super admin permissions.
	public func removeSuperAdmin(inboxId: InboxId) async throws {
		try await ffiGroup.removeSuperAdmin(inboxId: inboxId)
	}

	/// Returns the inbox IDs of all members with admin privileges.
	public func listAdmins() throws -> [InboxId] {
		try ffiGroup.adminList()
	}

	/// Returns the inbox IDs of all members with super admin privileges.
	public func listSuperAdmins() throws -> [InboxId] {
		try ffiGroup.superAdminList()
	}

	/// Returns the current permission policy set for this group.
	///
	/// The policy set defines which roles are allowed to perform actions such as
	/// adding or removing members, updating metadata, and modifying permissions.
	public func permissionPolicySet() throws -> PermissionPolicySet {
		try PermissionPolicySet.fromFfiPermissionPolicySet(
			permissions().policySet()
		)
	}

	/// Returns the inbox ID of the original creator of this group.
	public func creatorInboxId() async throws -> InboxId {
		try await metadata().creatorInboxId()
	}

	/// Returns the inbox ID of the member who added the current client to this group.
	public func addedByInboxId() throws -> InboxId {
		try ffiGroup.addedByInboxId()
	}

	/// The list of current group members.
	public var members: [Member] {
		get async throws {
			try await ffiGroup.listMembers().map { ffiGroupMember in
				Member(ffiGroupMember: ffiGroupMember)
			}
		}
	}

	/// The current client's membership state within this group.
	public var membershipState: GroupMembershipState {
		get throws {
			try ffiGroup.membershipState().fromFFI
		}
	}

	/// The inbox IDs of all group members except the current client.
	public var peerInboxIds: [InboxId] {
		get async throws {
			var ids = try await members.map(\.inboxId)
			if let index = ids.firstIndex(of: client.inboxID) {
				ids.remove(at: index)
			}
			return ids
		}
	}

	/// The date the group was created.
	public var createdAt: Date {
		Date(millisecondsSinceEpoch: ffiGroup.createdAtNs())
	}

	/// The group creation timestamp in nanoseconds since the Unix epoch.
	public var createdAtNs: Int64 {
		ffiGroup.createdAtNs()
	}

	/// The timestamp of the last activity in nanoseconds since the Unix epoch.
	///
	/// Returns the sent timestamp of the last message, or the group creation
	/// timestamp if no messages exist.
	public var lastActivityAtNs: Int64 {
		ffiLastMessage?.sentAtNs ?? createdAtNs
	}

	/// Adds members to the group by their inbox IDs.
	///
	/// - Parameter inboxIds: The inbox IDs of the members to add.
	/// - Returns: A ``GroupMembershipResult`` describing the outcome of the operation.
	/// - Throws: If any inbox ID is invalid or the caller lacks permission.
	/// - Important: Requires admin permissions unless the group's add-member policy allows all members.
	public func addMembers(inboxIds: [InboxId]) async throws
		-> GroupMembershipResult
	{
		try validateInboxIds(inboxIds)
		let result = try await ffiGroup.addMembers(inboxIds: inboxIds)
		return GroupMembershipResult(ffiGroupMembershipResult: result)
	}

	/// Removes members from the group by their inbox IDs.
	///
	/// - Parameter inboxIds: The inbox IDs of the members to remove.
	/// - Throws: If any inbox ID is invalid or the caller lacks permission.
	/// - Important: Requires admin permissions unless the group's remove-member policy allows all members.
	public func removeMembers(inboxIds: [InboxId]) async throws {
		try validateInboxIds(inboxIds)
		try await ffiGroup.removeMembers(inboxIds: inboxIds)
	}

	/// Adds members to the group by their public identities.
	///
	/// Use this method when you have ``PublicIdentity`` values instead of raw inbox IDs.
	///
	/// - Parameter identities: The public identities of the members to add.
	/// - Returns: A ``GroupMembershipResult`` describing the outcome of the operation.
	/// - Important: Requires admin permissions unless the group's add-member policy allows all members.
	public func addMembersByIdentity(identities: [PublicIdentity]) async throws
		-> GroupMembershipResult
	{
		let result = try await ffiGroup.addMembersByIdentity(
			accountIdentifiers: identities.map(\.ffiPrivate)
		)
		return GroupMembershipResult(ffiGroupMembershipResult: result)
	}

	/// Removes members from the group by their public identities.
	///
	/// Use this method when you have ``PublicIdentity`` values instead of raw inbox IDs.
	///
	/// - Parameter identities: The public identities of the members to remove.
	/// - Important: Requires admin permissions unless the group's remove-member policy allows all members.
	public func removeMembersByIdentity(identities: [PublicIdentity])
		async throws
	{
		try await ffiGroup.removeMembersByIdentity(
			accountIdentifiers: identities.map(\.ffiPrivate)
		)
	}

	/// Returns the display name of the group.
	public func name() throws -> String {
		try ffiGroup.groupName()
	}

	/// Returns the URL of the group's square image.
	public func imageUrl() throws -> String {
		try ffiGroup.groupImageUrlSquare()
	}

	/// Returns the group's description text.
	public func description() throws -> String {
		try ffiGroup.groupDescription()
	}

	/// Returns the group's custom application data string.
	public func appData() throws -> String {
		try ffiGroup.appData()
	}

	/// Updates the display name of the group.
	///
	/// - Parameter name: The new group name.
	/// - Important: Requires the appropriate metadata update permission.
	public func updateName(name: String) async throws {
		try await ffiGroup.updateGroupName(groupName: name)
	}

	/// Updates the group's square image URL.
	///
	/// - Parameter imageUrl: The new image URL.
	/// - Important: Requires the appropriate metadata update permission.
	public func updateImageUrl(imageUrl: String) async throws {
		try await ffiGroup.updateGroupImageUrlSquare(
			groupImageUrlSquare: imageUrl
		)
	}

	/// Updates the group's description text.
	///
	/// - Parameter description: The new description.
	/// - Important: Requires the appropriate metadata update permission.
	public func updateDescription(description: String) async throws {
		try await ffiGroup.updateGroupDescription(
			groupDescription: description
		)
	}

	/// Updates the group's custom application data.
	///
	/// - Parameter appData: The new application data string.
	/// - Important: Requires the appropriate metadata update permission.
	public func updateAppData(appData: String) async throws {
		try await ffiGroup.updateAppData(appData: appData)
	}

	/// Updates the permission policy for adding members.
	///
	/// - Parameter newPermissionOption: The new permission level required to add members.
	/// - Important: Requires super admin permissions.
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

	/// Updates the permission policy for removing members.
	///
	/// - Parameter newPermissionOption: The new permission level required to remove members.
	/// - Important: Requires super admin permissions.
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

	/// Updates the permission policy for adding admins.
	///
	/// - Parameter newPermissionOption: The new permission level required to add admins.
	/// - Important: Requires super admin permissions.
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

	/// Updates the permission policy for removing admins.
	///
	/// - Parameter newPermissionOption: The new permission level required to remove admins.
	/// - Important: Requires super admin permissions.
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

	/// Updates the permission policy for changing the group name.
	///
	/// - Parameter newPermissionOption: The new permission level required to update the name.
	/// - Important: Requires super admin permissions.
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

	/// Updates the permission policy for changing the group description.
	///
	/// - Parameter newPermissionOption: The new permission level required to update the description.
	/// - Important: Requires super admin permissions.
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

	/// Updates the permission policy for changing the group image URL.
	///
	/// - Parameter newPermissionOption: The new permission level required to update the image URL.
	/// - Important: Requires super admin permissions.
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

	/// Updates the disappearing message settings for this group.
	///
	/// Pass `nil` to clear (disable) disappearing messages.
	///
	/// - Parameter disappearingMessageSettings: The new settings, or `nil` to disable.
	/// - Important: Requires the appropriate permission to update disappearing message settings.
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

	/// Disables disappearing messages for this group by removing the current settings.
	///
	/// - Important: Requires the appropriate permission to update disappearing message settings.
	public func clearDisappearingMessageSettings() async throws {
		try await ffiGroup.removeConversationMessageDisappearingSettings()
	}

	/// Returns null if group is not paused, otherwise the min version required to unpause this group
	public func pausedForVersion() throws -> String? {
		try ffiGroup.pausedForVersion()
	}

	/// Updates the local consent state for this group.
	///
	/// Use this to mark a group as allowed, denied, or unknown for consent filtering.
	///
	/// - Parameter state: The new consent state.
	public func updateConsentState(state: ConsentState) async throws {
		try ffiGroup.updateConsentState(state: state.toFFI)
	}

	/// Returns the current local consent state for this group.
	public func consentState() throws -> ConsentState {
		try ffiGroup.consentState().fromFFI
	}

	/// Processes an incoming push notification payload into a decoded message.
	///
	/// Use this to decrypt and decode a message received via push notification
	/// without needing to perform a full ``sync()``.
	///
	/// - Parameter messageBytes: The raw message bytes from the push notification.
	/// - Returns: A ``DecodedMessage`` if the payload could be decoded, or `nil` if the message was not processable.
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

	/// Sends a message to the group.
	///
	/// The content is encoded using the codec matching the content type
	/// specified in ``SendOptions``, or the default text codec if none is specified.
	///
	/// - Parameters:
	///   - content: The message content to send (e.g., a `String` for text messages).
	///   - options: Optional send options including content type and compression.
	/// - Returns: The hex-encoded ID of the sent message.
	/// - Throws: ``CodecError/invalidContent`` if the content cannot be encoded by the selected codec.
	public func send(content: some Any, options: SendOptions? = nil) async throws
		-> String
	{
		let (encodeContent, visibilityOptions) = try await encodeContent(
			content: content, options: options
		)
		return try await send(encodedContent: encodeContent, visibilityOptions: visibilityOptions)
	}

	/// Sends a pre-encoded message to the group.
	///
	/// Use this when you have already encoded the content with a codec and want
	/// to send the raw ``EncodedContent`` directly.
	///
	/// - Parameters:
	///   - encodedContent: The pre-encoded message content.
	///   - visibilityOptions: Optional visibility options such as whether to trigger a push notification.
	/// - Returns: The hex-encoded ID of the sent message.
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

	/// Encodes content using the appropriate codec without sending it.
	///
	/// Useful when you need to inspect or modify the encoded payload before sending,
	/// or when building optimistic-send workflows with ``prepareMessage(encodedContent:visibilityOptions:noSend:)``.
	///
	/// - Parameters:
	///   - content: The message content to encode.
	///   - options: Optional send options specifying the content type and compression.
	/// - Returns: A tuple of the encoded content and the resolved visibility options.
	/// - Throws: ``CodecError/invalidContent`` if the content cannot be encoded by the selected codec.
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

	/// Prepares a pre-encoded message for sending, enabling optimistic UI patterns.
	///
	/// When `noSend` is `false` (the default), the message is stored locally and queued for
	/// network delivery, allowing you to display it in the UI immediately. Call
	/// ``publishMessages()`` later to flush the send queue.
	///
	/// When `noSend` is `true`, the message is only stored locally and will not
	/// be published until you explicitly call ``publishMessages()``.
	///
	/// - Parameters:
	///   - encodedContent: The pre-encoded message content.
	///   - visibilityOptions: Optional visibility options such as whether to trigger a push notification.
	///   - noSend: If `true`, the message is stored locally but not queued for sending. Defaults to `false`.
	/// - Returns: The hex-encoded ID of the prepared message.
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

	/// Encodes and prepares a message for sending, enabling optimistic UI patterns.
	///
	/// This is a convenience that combines ``encodeContent(content:options:)`` and
	/// ``prepareMessage(encodedContent:visibilityOptions:noSend:)``.
	///
	/// - Parameters:
	///   - content: The message content to encode and prepare.
	///   - options: Optional send options including content type and compression.
	///   - noSend: If `true`, the message is stored locally but not queued for sending. Defaults to `false`.
	/// - Returns: The hex-encoded ID of the prepared message.
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

	/// Publishes all pending prepared messages to the network.
	///
	/// Call this after ``prepareMessage(encodedContent:visibilityOptions:noSend:)`` to flush the
	/// local send queue and deliver messages to the group.
	public func publishMessages() async throws {
		try await ffiGroup.publishMessages()
	}

	/// Publishes a single prepared message to the network by its message ID.
	///
	/// - Parameter messageId: The hex-encoded ID of the prepared message to publish.
	public func publishMessage(messageId: String) async throws {
		try await ffiGroup.publishStoredMessage(messageId: messageId.hexToData)
	}

	/// Ends the active message stream for this group, if one exists.
	public func endStream() {
		streamHolder.stream?.end()
	}

	/// Returns an asynchronous stream of new messages arriving in this group in real time.
	///
	/// The stream will continue producing messages until ``endStream()`` is called,
	/// the task is cancelled, or an error occurs.
	///
	/// - Parameter onClose: An optional closure called when the stream closes.
	/// - Returns: An `AsyncThrowingStream` that yields ``DecodedMessage`` values as they arrive.
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

	/// Returns the most recent message in the group, or `nil` if the group has no messages.
	///
	/// Uses a cached last message if available; otherwise queries the local store.
	public func lastMessage() async throws -> DecodedMessage? {
		if let ffiMessage = ffiLastMessage {
			DecodedMessage.create(ffiMessage: ffiMessage)
		} else {
			try await messages(limit: 1).first
		}
	}

	/// Returns the commit log fork status for this group.
	///
	/// A forked commit log indicates the group's MLS state has diverged
	/// and may need recovery.
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

	/// Returns messages with their associated reaction messages included.
	///
	/// Each returned ``DecodedMessage`` includes child reactions as part of its structure.
	/// For a more comprehensive approach that also includes replies and other enriched metadata,
	/// consider using ``enrichedMessages(beforeNs:afterNs:limit:direction:deliveryStatus:excludeContentTypes:excludeSenderInboxIds:sortBy:insertedAfterNs:insertedBeforeNs:)`` instead.
	///
	/// - Parameters:
	///   - beforeNs: Only include messages sent before this timestamp (nanoseconds since epoch).
	///   - afterNs: Only include messages sent after this timestamp (nanoseconds since epoch).
	///   - limit: Maximum number of messages to return.
	///   - direction: Sort order; defaults to ``SortDirection/descending`` (newest first).
	///   - deliveryStatus: Filter by delivery status; defaults to `.all`.
	///   - excludeContentTypes: Content types to exclude from results.
	///   - excludeSenderInboxIds: Sender inbox IDs to exclude from results.
	///   - sortBy: The field to sort results by.
	///   - insertedAfterNs: Only include messages inserted into the local database after this timestamp.
	///   - insertedBeforeNs: Only include messages inserted into the local database before this timestamp.
	/// - Returns: An array of ``DecodedMessage`` values with reactions attached.
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

	/// Returns the count of messages in this group matching the given filters.
	///
	/// - Parameters:
	///   - beforeNs: Only count messages sent before this timestamp (nanoseconds since epoch).
	///   - afterNs: Only count messages sent after this timestamp (nanoseconds since epoch).
	///   - deliveryStatus: Filter by delivery status; defaults to `.all`.
	///   - excludeContentTypes: Content types to exclude from the count.
	///   - excludeSenderInboxIds: Sender inbox IDs to exclude from the count.
	///   - insertedAfterNs: Only count messages inserted into the local database after this timestamp.
	///   - insertedBeforeNs: Only count messages inserted into the local database before this timestamp.
	/// - Returns: The number of matching messages.
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

	/// Returns the HMAC keys for this group, used for push notification decryption.
	///
	/// The keys are keyed by topic string with epoch-based rotation.
	/// - Returns: An HMAC keys response containing key data for each topic.
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

	/// Returns the topic strings to subscribe to for push notifications for this group.
	public func getPushTopics() throws -> [String] {
		[topic]
	}

	/// Returns debug information about this group's internal state.
	///
	/// Useful for diagnostics and troubleshooting MLS state issues.
	public func getDebugInformation() async throws -> ConversationDebugInfo {
		try await ConversationDebugInfo(
			ffiConversationDebugInfo: ffiGroup.conversationDebugInfo()
		)
	}

	/// Returns the last read timestamps for members of this group.
	///
	/// - Returns: A dictionary mapping inbox IDs to their last read timestamp in nanoseconds since epoch.
	public func getLastReadTimes() throws -> [String: Int64] {
		try ffiGroup.getLastReadTimes()
	}

	/// Removes the current client from this group.
	///
	/// After leaving, the group will become inactive for this client and
	/// ``isActive()`` will return `false`.
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
