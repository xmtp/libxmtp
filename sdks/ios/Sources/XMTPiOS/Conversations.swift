import Foundation

/// Errors that can occur when creating or interacting with conversations.
public enum ConversationError: Error, CustomStringConvertible, LocalizedError {
	/// The caller attempted to create a conversation with themselves.
	case memberCannotBeSelf
	/// One or more members are not registered on the XMTP network.
	///
	/// The associated value contains the unregistered member identifiers.
	case memberNotRegistered([String])
	/// A group operation was called without providing the required message payload.
	case groupsRequireMessagePassed
	/// The called method is not supported for group conversations.
	case notSupportedByGroups
	/// A real-time stream encountered an unrecoverable error and was closed.
	case streamingFailure

	public var description: String {
		switch self {
		case .memberCannotBeSelf:
			"GroupError.memberCannotBeSelf you cannot add yourself to a group"
		case let .memberNotRegistered(array):
			"GroupError.memberNotRegistered members not registered: \(array.joined(separator: ", "))"
		case .groupsRequireMessagePassed:
			"GroupError.groupsRequireMessagePassed you cannot call this method without passing a message instead of an envelope"
		case .notSupportedByGroups:
			"GroupError.notSupportedByGroups this method is not supported by groups"
		case .streamingFailure:
			"GroupError.streamingFailure a stream has failed"
		}
	}

	public var errorDescription: String? {
		description
	}
}

/// A summary of the results from syncing all conversations.
///
/// After calling ``Conversations/syncAllConversations(consentStates:)``, this struct
/// reports how many conversations were eligible for syncing and how many were
/// actually synced.
public struct GroupSyncSummary {
	/// The total number of conversations that were eligible to be synced.
	public var numEligible: UInt64
	/// The number of conversations that were successfully synced.
	public var numSynced: UInt64

	public init(numEligible: UInt64, numSynced: UInt64) {
		self.numEligible = numEligible
		self.numSynced = numSynced
	}

	init(ffiGroupSyncSummary: FfiGroupSyncSummary) {
		numEligible = ffiGroupSyncSummary.numEligible
		numSynced = ffiGroupSyncSummary.numSynced
	}
}

/// Filters the type of conversations returned by list and stream methods.
public enum ConversationFilterType {
	/// Include both group conversations and DMs.
	case all
	/// Include only group conversations.
	case groups
	/// Include only direct message (DM) conversations.
	case dms
}

/// Controls the sort order of conversations returned by list methods.
public enum ConversationsOrderBy {
	/// Order by conversation creation timestamp.
	case createdAt
	/// Order by the most recent activity (message sent or received) in the conversation.
	case lastActivity

	fileprivate var ffiOrderBy: FfiGroupQueryOrderBy {
		switch self {
		case .createdAt: .createdAt
		case .lastActivity: .lastActivity
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
		onCloseCallback = onClose
	}

	func onClose() {
		onCloseCallback()
	}

	func onError(error: FfiError) {
		print("Error ConversationStreamCallback \(error)")
	}

	func onConversation(conversation: FfiConversation) {
		callback(conversation)
	}
}

final class MessageDeletionCallback: FfiMessageDeletionCallback {
	let onCloseCallback: () -> Void
	let callback: (FfiDecodedMessage) -> Void

	init(
		callback: @escaping (FfiDecodedMessage) -> Void,
		onClose: @escaping () -> Void
	) {
		self.callback = callback
		onCloseCallback = onClose
	}

	func onClose() {
		onCloseCallback()
	}

	func onError(error: FfiError) {
		print("Error MessageDeletionCallback \(error)")
	}

	func onMessageDeleted(message: FfiDecodedMessage) {
		callback(message)
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
	private func toFfiDisappearingMessageSettings(_ settings: DisappearingMessageSettings?)
		-> FfiMessageDisappearingSettings?
	{
		guard let settings else { return nil }
		return FfiMessageDisappearingSettings(
			fromNs: settings.disappearStartingAtNs,
			inNs: settings.retentionDurationInNs
		)
	}

	/// Finds a group conversation by its unique identifier.
	///
	/// Looks up a group in the local database. Call ``sync()`` first to ensure the
	/// local state is up to date with the network.
	///
	/// - Parameter groupId: The hex-encoded group identifier.
	/// - Returns: The ``Group`` if found, or `nil` if no group matches the given ID.
	public func findGroup(groupId: String) throws -> Group? {
		do {
			return try Group(
				ffiGroup: ffiClient.conversation(
					conversationId: groupId.hexToData
				),
				client: client
			)
		} catch {
			return nil
		}
	}

	/// Finds any conversation (group or DM) by its unique identifier.
	///
	/// Looks up a conversation in the local database. Call ``sync()`` first to ensure
	/// the local state is up to date with the network.
	///
	/// - Parameter conversationId: The hex-encoded conversation identifier.
	/// - Returns: The ``Conversation`` if found, or `nil` if no conversation matches.
	public func findConversation(conversationId: String) async throws
		-> Conversation?
	{
		do {
			let conversation = try ffiClient.conversation(
				conversationId: conversationId.hexToData
			)
			return try await conversation.toConversation(client: client)
		} catch {
			return nil
		}
	}

	/// Finds a conversation by its topic string.
	///
	/// Extracts the conversation ID from an XMTP topic string of the form
	/// `/xmtp/mls/1/g-<conversationId>/proto` and looks up the conversation locally.
	///
	/// - Parameter topic: The full XMTP topic string.
	/// - Returns: The ``Conversation`` if found, or `nil` if the topic does not match
	///   the expected format or no conversation exists for the extracted ID.
	public func findConversationByTopic(topic: String) async throws
		-> Conversation?
	{
		do {
			let regexPattern = #"/xmtp/mls/1/g-(.*?)/proto"#
			if let regex = try? NSRegularExpression(pattern: regexPattern) {
				let range = NSRange(location: 0, length: topic.utf16.count)
				if let match = regex.firstMatch(
					in: topic, options: [], range: range
				) {
					let conversationId = (topic as NSString).substring(
						with: match.range(at: 1)
					)
					let conversation = try ffiClient.conversation(
						conversationId: conversationId.hexToData
					)
					return try await conversation.toConversation(client: client)
				}
			}
		} catch {
			return nil
		}
		return nil
	}

	/// Finds an existing DM conversation with the given inbox ID.
	///
	/// Searches the local database for a direct message conversation with the
	/// specified peer. Call ``sync()`` first to ensure the local state is current.
	///
	/// - Parameter inboxId: The inbox ID of the other participant.
	/// - Returns: The ``Dm`` if found, or `nil` if no DM exists with that peer.
	public func findDmByInboxId(inboxId: InboxId) throws -> Dm? {
		do {
			let conversation = try ffiClient.dmConversation(
				targetInboxId: inboxId
			)
			return Dm(
				ffiConversation: conversation, client: client
			)
		} catch {
			return nil
		}
	}

	/// Finds an existing DM conversation with the given public identity.
	///
	/// Resolves the identity to an inbox ID, then searches the local database for
	/// a DM with that peer.
	///
	/// - Parameter publicIdentity: The ``PublicIdentity`` of the other participant.
	/// - Returns: The ``Dm`` if found, or `nil` if no DM exists with that peer.
	/// - Throws: ``ClientError/creationError(_:)`` if the identity cannot be resolved
	///   to an inbox ID.
	public func findDmByIdentity(publicIdentity: PublicIdentity) async throws
		-> Dm?
	{
		guard
			let inboxId = try await client.inboxIdFromIdentity(
				identity: publicIdentity
			)
		else {
			throw ClientError.creationError("No inboxId present")
		}
		return try findDmByInboxId(inboxId: inboxId)
	}

	/// Finds a message by its unique identifier.
	///
	/// Looks up a single message in the local database.
	///
	/// - Parameter messageId: The hex-encoded message identifier.
	/// - Returns: The ``DecodedMessage`` if found, or `nil` if no message matches.
	public func findMessage(messageId: String) throws -> DecodedMessage? {
		do {
			return try DecodedMessage.create(
				ffiMessage: ffiClient.message(
					messageId: messageId.hexToData
				)
			)
		} catch {
			return nil
		}
	}

	/// Finds an enriched message by its unique identifier.
	///
	/// An enriched message includes additional metadata beyond the standard
	/// ``DecodedMessage``, such as delivery and read status.
	///
	/// - Parameter messageId: The hex-encoded message identifier.
	/// - Returns: The ``DecodedMessageV2`` if found, or `nil` if no message matches.
	public func findEnrichedMessage(messageId: String) throws -> DecodedMessageV2? {
		do {
			return try DecodedMessageV2.create(
				ffiMessage: ffiClient.enrichedMessage(messageId: messageId.hexToData)
			)
		} catch {
			return nil
		}
	}

	/// Delete a message from your local database. Does not impact other devices or installations
	public func deleteMessageLocally(messageId: String) throws {
		_ = try ffiClient.deleteMessage(messageId: messageId.hexToData)
	}

	/// Syncs the conversation list from the network.
	///
	/// Downloads any new or updated conversations (groups and DMs) from the XMTP
	/// network and stores them locally. This does **not** sync the messages within
	/// each conversation; use ``syncAllConversations(consentStates:)`` for that.
	///
	/// Call this before listing or finding conversations to ensure the local
	/// database reflects the latest network state.
	///
	/// - Throws: If the network request fails.
	public func sync() async throws {
		try await ffiConversations.sync()
	}

	/// Syncs messages for all conversations from the network.
	///
	/// Downloads new messages for every conversation and stores them locally.
	/// Unlike ``sync()``, which only updates the conversation list, this method
	/// fetches message content for each conversation.
	///
	/// - Parameter consentStates: An optional filter to only sync conversations matching
	///   the given consent states (e.g., `.allowed`). Pass `nil` to sync all conversations.
	/// - Returns: A ``GroupSyncSummary`` indicating how many conversations were eligible
	///   and how many were successfully synced.
	/// - Throws: If the network request fails.
	public func syncAllConversations(consentStates: [ConsentState]? = nil)
		async throws -> GroupSyncSummary
	{
		let ffiResult = try await ffiConversations.syncAllConversations(
			consentStates: consentStates?.toFFI
		)
		return GroupSyncSummary(ffiGroupSyncSummary: ffiResult)
	}

	/// Lists group conversations from the local database.
	///
	/// Returns groups matching the specified filters, sorted by the given order.
	/// Call ``sync()`` first to ensure the local database is up to date.
	///
	/// - Parameters:
	///   - createdAfterNs: Only include groups created after this timestamp (in nanoseconds).
	///   - createdBeforeNs: Only include groups created before this timestamp (in nanoseconds).
	///   - lastActivityAfterNs: Only include groups with activity after this timestamp (in nanoseconds).
	///   - lastActivityBeforeNs: Only include groups with activity before this timestamp (in nanoseconds).
	///   - limit: Maximum number of groups to return.
	///   - consentStates: Filter by consent states (e.g., `.allowed`, `.denied`).
	///   - orderBy: Sort order for the results. Defaults to ``ConversationsOrderBy/lastActivity``.
	/// - Returns: An array of ``Group`` objects matching the filters.
	/// - Throws: If the local database query fails.
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

	/// Lists direct message (DM) conversations from the local database.
	///
	/// Returns DMs matching the specified filters, sorted by the given order.
	/// Call ``sync()`` first to ensure the local database is up to date.
	///
	/// - Parameters:
	///   - createdAfterNs: Only include DMs created after this timestamp (in nanoseconds).
	///   - createdBeforeNs: Only include DMs created before this timestamp (in nanoseconds).
	///   - lastActivityBeforeNs: Only include DMs with activity before this timestamp (in nanoseconds).
	///   - lastActivityAfterNs: Only include DMs with activity after this timestamp (in nanoseconds).
	///   - limit: Maximum number of DMs to return.
	///   - consentStates: Filter by consent states (e.g., `.allowed`, `.denied`).
	///   - orderBy: Sort order for the results. Defaults to ``ConversationsOrderBy/lastActivity``.
	/// - Returns: An array of ``Dm`` objects matching the filters.
	/// - Throws: If the local database query fails.
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

	/// Lists all conversations (groups and DMs) from the local database.
	///
	/// Returns conversations matching the specified filters, sorted by the given order.
	/// Call ``sync()`` first to ensure the local database is up to date.
	///
	/// - Parameters:
	///   - createdAfterNs: Only include conversations created after this timestamp (in nanoseconds).
	///   - createdBeforeNs: Only include conversations created before this timestamp (in nanoseconds).
	///   - lastActivityBeforeNs: Only include conversations with activity before this timestamp (in nanoseconds).
	///   - lastActivityAfterNs: Only include conversations with activity after this timestamp (in nanoseconds).
	///   - limit: Maximum number of conversations to return.
	///   - consentStates: Filter by consent states (e.g., `.allowed`, `.denied`).
	///   - orderBy: Sort order for the results. Defaults to ``ConversationsOrderBy/lastActivity``.
	/// - Returns: An array of ``Conversation`` values matching the filters.
	/// - Throws: If the local database query fails.
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

	/// Streams new conversations in real time.
	///
	/// Returns an `AsyncThrowingStream` that yields each new ``Conversation``
	/// (group or DM) as it is created on the network. The stream remains open
	/// until cancelled or an error occurs.
	///
	/// - Parameters:
	///   - type: The kind of conversations to stream. Defaults to ``ConversationFilterType/all``.
	///   - onClose: An optional closure invoked when the stream is closed by the server.
	/// - Returns: An `AsyncThrowingStream` of ``Conversation`` values.
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
									conversation.dmFromFFI(client: self.client)
								)
							)
						} else if conversationType == .group {
							continuation.yield(
								Conversation.group(
									conversation.groupFromFFI(
										client: self.client
									)
								)
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
				let stream: FfiStreamCloser = switch type {
				case .groups:
					await ffiConversations.streamGroups(
						callback: conversationCallback
					)
				case .all:
					await ffiConversations.stream(
						callback: conversationCallback
					)
				case .dms:
					await ffiConversations.streamDms(
						callback: conversationCallback
					)
				}
				await ffiStreamActor.setFfiStream(stream)
				continuation.onTermination = { @Sendable _ in
					Task {
						await ffiStreamActor.endStream()
					}
				}
			}

			continuation.onTermination = { @Sendable _ in
				task.cancel()
				Task {
					await ffiStreamActor.endStream()
				}
			}
		}
	}

	/// Finds or creates a DM conversation with the given public identity.
	///
	/// If a DM already exists with the peer, it is returned. Otherwise a new DM
	/// is created. This method never creates duplicate conversations.
	///
	/// - Parameters:
	///   - peerIdentity: The ``PublicIdentity`` of the peer to message.
	///   - disappearingMessageSettings: Optional settings for disappearing messages in this conversation.
	/// - Returns: The existing or newly created ``Conversation`` (always a `.dm`).
	/// - Throws: ``ConversationError/memberCannotBeSelf`` if the identity belongs to the current user.
	public func newConversationWithIdentity(
		with peerIdentity: PublicIdentity,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Conversation {
		let dm = try await findOrCreateDmWithIdentity(
			with: peerIdentity,
			disappearingMessageSettings: disappearingMessageSettings
		)
		return Conversation.dm(dm)
	}

	/// Finds or creates a DM with the given public identity, returning the ``Dm`` directly.
	///
	/// If a DM already exists with the peer, it is returned. Otherwise a new DM
	/// is created. This method never creates duplicate conversations.
	///
	/// - Parameters:
	///   - peerIdentity: The ``PublicIdentity`` of the peer to message.
	///   - disappearingMessageSettings: Optional settings for disappearing messages in this conversation.
	/// - Returns: The existing or newly created ``Dm``.
	/// - Throws: ``ConversationError/memberCannotBeSelf`` if the identity belongs to the current user.
	public func findOrCreateDmWithIdentity(
		with peerIdentity: PublicIdentity,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Dm {
		if try await client.inboxState(refreshFromNetwork: false).identities
			.map(\.identifier).contains(peerIdentity.identifier)
		{
			throw ConversationError.memberCannotBeSelf
		}

		let dm =
			try await ffiConversations
				.findOrCreateDmByIdentity(
					targetIdentity: peerIdentity.ffiPrivate,
					opts: FfiCreateDmOptions(
						messageDisappearingSettings: toFfiDisappearingMessageSettings(
							disappearingMessageSettings
						)
					)
				)

		return dm.dmFromFFI(client: client)
	}

	/// Finds or creates a DM conversation with the given inbox ID.
	///
	/// If a DM already exists with the peer, it is returned. Otherwise a new DM
	/// is created. This method never creates duplicate conversations.
	///
	/// - Parameters:
	///   - peerInboxId: The inbox ID of the peer to message.
	///   - disappearingMessageSettings: Optional settings for disappearing messages in this conversation.
	/// - Returns: The existing or newly created ``Conversation`` (always a `.dm`).
	/// - Throws: ``ConversationError/memberCannotBeSelf`` if the inbox ID matches the current user.
	public func newConversation(
		with peerInboxId: InboxId,
		disappearingMessageSettings: DisappearingMessageSettings? = nil
	) async throws -> Conversation {
		let dm = try await findOrCreateDm(
			with: peerInboxId,
			disappearingMessageSettings: disappearingMessageSettings
		)
		return Conversation.dm(dm)
	}

	/// Finds or creates a DM with the given inbox ID, returning the ``Dm`` directly.
	///
	/// If a DM already exists with the peer, it is returned. Otherwise a new DM
	/// is created. This method never creates duplicate conversations.
	///
	/// - Parameters:
	///   - peerInboxId: The inbox ID of the peer to message.
	///   - disappearingMessageSettings: Optional settings for disappearing messages in this conversation.
	/// - Returns: The existing or newly created ``Dm``.
	/// - Throws: ``ConversationError/memberCannotBeSelf`` if the inbox ID matches the current user.
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
				.findOrCreateDm(
					inboxId: peerInboxId,
					opts: FfiCreateDmOptions(
						messageDisappearingSettings: toFfiDisappearingMessageSettings(
							disappearingMessageSettings
						)
					)
				)
		return dm.dmFromFFI(client: client)
	}

	/// Creates a new group conversation with the given public identities.
	///
	/// A new group is created every time this method is called, even if a group with the
	/// same members already exists. Use one of the preconfigured permission levels.
	///
	/// - Parameters:
	///   - identities: The ``PublicIdentity`` values of the members to add.
	///   - permissions: The permission preset for the group. Defaults to ``GroupPermissionPreconfiguration/allMembers``.
	///   - name: An optional display name for the group.
	///   - imageUrl: An optional URL for the group's avatar image.
	///   - description: An optional description of the group.
	///   - disappearingMessageSettings: Optional settings for disappearing messages.
	///   - appData: Optional application-specific metadata.
	/// - Returns: The newly created ``Group``.
	/// - Throws: If any identity is not registered on the XMTP network.
	public func newGroupWithIdentities(
		with identities: [PublicIdentity],
		permissions: GroupPermissionPreconfiguration = .allMembers,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil,
		appData: String? = nil
	) async throws -> Group {
		try await newGroupInternalWithIdentities(
			with: identities,
			permissions:
			GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
				option: permissions
			),
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: nil,
			disappearingMessageSettings: disappearingMessageSettings,
			appData: appData
		)
	}

	/// Creates a new group conversation with custom permissions and the given public identities.
	///
	/// A new group is created every time this method is called. Use this variant when the
	/// preconfigured permission levels do not meet your needs.
	///
	/// - Parameters:
	///   - identities: The ``PublicIdentity`` values of the members to add.
	///   - permissionPolicySet: A ``PermissionPolicySet`` defining fine-grained permissions.
	///   - name: An optional display name for the group.
	///   - imageUrl: An optional URL for the group's avatar image.
	///   - description: An optional description of the group.
	///   - disappearingMessageSettings: Optional settings for disappearing messages.
	///   - appData: Optional application-specific metadata.
	/// - Returns: The newly created ``Group``.
	/// - Throws: If any identity is not registered on the XMTP network.
	public func newGroupCustomPermissionsWithIdentities(
		with identities: [PublicIdentity],
		permissionPolicySet: PermissionPolicySet,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil,
		appData: String? = nil
	) async throws -> Group {
		try await newGroupInternalWithIdentities(
			with: identities,
			permissions: FfiGroupPermissionsOptions.customPolicy,
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: PermissionPolicySet.toFfiPermissionPolicySet(
				permissionPolicySet
			),
			disappearingMessageSettings: disappearingMessageSettings,
			appData: appData
		)
	}

	private func newGroupInternalWithIdentities(
		with identities: [PublicIdentity],
		permissions: FfiGroupPermissionsOptions = .default,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		permissionPolicySet: FfiPermissionPolicySet? = nil,
		disappearingMessageSettings: DisappearingMessageSettings? = nil,
		appData: String?
	) async throws -> Group {
		try await ffiConversations.createGroupByIdentity(
			accountIdentities: identities.map(\.ffiPrivate),
			opts: FfiCreateGroupOptions(
				permissions: permissions,
				groupName: name,
				groupImageUrlSquare: imageUrl,
				groupDescription: description,
				customPermissionPolicySet: permissionPolicySet,
				messageDisappearingSettings: toFfiDisappearingMessageSettings(
					disappearingMessageSettings
				),
				appData: appData
			)
		).groupFromFFI(client: client)
	}

	/// Creates a new group conversation with the given inbox IDs.
	///
	/// A new group is created every time this method is called, even if a group with the
	/// same members already exists. Use one of the preconfigured permission levels.
	///
	/// - Parameters:
	///   - inboxIds: The inbox IDs of the members to add.
	///   - permissions: The permission preset for the group. Defaults to ``GroupPermissionPreconfiguration/allMembers``.
	///   - name: An optional display name for the group.
	///   - imageUrl: An optional URL for the group's avatar image.
	///   - description: An optional description of the group.
	///   - disappearingMessageSettings: Optional settings for disappearing messages.
	///   - appData: Optional application-specific metadata.
	/// - Returns: The newly created ``Group``.
	/// - Throws: If any inbox ID is invalid or not registered on the XMTP network.
	public func newGroup(
		with inboxIds: [InboxId],
		permissions: GroupPermissionPreconfiguration = .allMembers,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil,
		appData: String? = nil
	) async throws -> Group {
		try await newGroupInternal(
			with: inboxIds,
			permissions:
			GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
				option: permissions
			),
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: nil,
			disappearingMessageSettings: disappearingMessageSettings,
			appData: appData
		)
	}

	/// Creates a new group conversation with custom permissions and the given inbox IDs.
	///
	/// A new group is created every time this method is called. Use this variant when the
	/// preconfigured permission levels do not meet your needs.
	///
	/// - Parameters:
	///   - inboxIds: The inbox IDs of the members to add.
	///   - permissionPolicySet: A ``PermissionPolicySet`` defining fine-grained permissions.
	///   - name: An optional display name for the group.
	///   - imageUrl: An optional URL for the group's avatar image.
	///   - description: An optional description of the group.
	///   - disappearingMessageSettings: Optional settings for disappearing messages.
	///   - appData: Optional application-specific metadata.
	/// - Returns: The newly created ``Group``.
	/// - Throws: If any inbox ID is invalid or not registered on the XMTP network.
	public func newGroupCustomPermissions(
		with inboxIds: [InboxId],
		permissionPolicySet: PermissionPolicySet,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil,
		appData: String? = nil
	) async throws -> Group {
		try await newGroupInternal(
			with: inboxIds,
			permissions: FfiGroupPermissionsOptions.customPolicy,
			name: name,
			imageUrl: imageUrl,
			description: description,
			permissionPolicySet: PermissionPolicySet.toFfiPermissionPolicySet(
				permissionPolicySet
			),
			disappearingMessageSettings: disappearingMessageSettings,
			appData: appData
		)
	}

	private func newGroupInternal(
		with inboxIds: [InboxId],
		permissions: FfiGroupPermissionsOptions = .default,
		name: String = "",
		imageUrl: String = "",
		description: String = "",
		permissionPolicySet: FfiPermissionPolicySet? = nil,
		disappearingMessageSettings: DisappearingMessageSettings? = nil,
		appData: String?
	) async throws -> Group {
		try validateInboxIds(inboxIds)
		return try await ffiConversations.createGroup(
			inboxIds: inboxIds,
			opts: FfiCreateGroupOptions(
				permissions: permissions,
				groupName: name,
				groupImageUrlSquare: imageUrl,
				groupDescription: description,
				customPermissionPolicySet: permissionPolicySet,
				messageDisappearingSettings: toFfiDisappearingMessageSettings(
					disappearingMessageSettings
				),
				appData: appData
			)
		).groupFromFFI(client: client)
	}

	/// Creates a new group optimistically without a network round-trip.
	///
	/// The group is created locally and the creator is the only member. Members must
	/// be added separately after creation (e.g., via `Group.addMembers`). This is
	/// useful for reducing latency in UIs that display the group immediately.
	///
	/// Because this is a local-only operation, the method is synchronous and does
	/// not require `await`.
	///
	/// - Parameters:
	///   - permissions: The permission preset for the group. Defaults to ``GroupPermissionPreconfiguration/allMembers``.
	///   - groupName: An optional display name for the group.
	///   - groupImageUrlSquare: An optional URL for the group's avatar image.
	///   - groupDescription: An optional description of the group.
	///   - disappearingMessageSettings: Optional settings for disappearing messages.
	///   - appData: Optional application-specific metadata.
	/// - Returns: The newly created ``Group`` with only the current user as a member.
	/// - Throws: If the local group creation fails.
	public func newGroupOptimistic(
		permissions: GroupPermissionPreconfiguration = .allMembers,
		groupName: String = "",
		groupImageUrlSquare: String = "",
		groupDescription: String = "",
		disappearingMessageSettings: DisappearingMessageSettings? = nil,
		appData: String? = nil
	) throws -> Group {
		let ffiOpts = FfiCreateGroupOptions(
			permissions:
			GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
				option: permissions
			),
			groupName: groupName,
			groupImageUrlSquare: groupImageUrlSquare,
			groupDescription: groupDescription,
			customPermissionPolicySet: nil,
			messageDisappearingSettings: toFfiDisappearingMessageSettings(
				disappearingMessageSettings
			),
			appData: appData
		)

		let ffiGroup = try ffiConversations.createGroupOptimistic(opts: ffiOpts)
		return Group(ffiGroup: ffiGroup, client: client)
	}

	/// Streams messages from all conversations in real time.
	///
	/// Returns an `AsyncThrowingStream` that yields each new ``DecodedMessage``
	/// as it arrives across all conversations. The stream remains open until
	/// cancelled or an error occurs.
	///
	/// - Parameters:
	///   - type: The kind of conversations to include. Defaults to ``ConversationFilterType/all``.
	///   - consentStates: An optional filter to only stream messages from conversations
	///     matching the given consent states. Pass `nil` to include all.
	///   - onClose: An optional closure invoked when the stream is closed by the server.
	/// - Returns: An `AsyncThrowingStream` of ``DecodedMessage`` values.
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
				let stream: FfiStreamCloser = switch type {
				case .groups:
					await ffiConversations.streamAllGroupMessages(
						messageCallback: messageCallback,
						consentStates: consentStates?.toFFI
					)
				case .dms:
					await ffiConversations.streamAllDmMessages(
						messageCallback: messageCallback,
						consentStates: consentStates?.toFFI
					)
				case .all:
					await ffiConversations.streamAllMessages(
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

	/// A stream of all deleted or disappeared messages
	/// that will be emitted as the messages are removed from the database
	public func streamMessageDeletions(
		onClose: (() -> Void)? = nil
	) -> AsyncThrowingStream<DecodedMessageV2, Error> {
		AsyncThrowingStream { continuation in
			let ffiStreamActor = FfiStreamActor()

			let deletionCallback = MessageDeletionCallback {
				ffiMessage in
				guard !Task.isCancelled else {
					continuation.finish()
					Task {
						await ffiStreamActor.endStream()
					}
					return
				}
				if let message = DecodedMessageV2(ffiMessage: ffiMessage) {
					continuation.yield(message)
				}
			} onClose: {
				onClose?()
				continuation.finish()
			}

			let task = Task {
				let stream = await ffiConversations.streamMessageDeletions(
					callback: deletionCallback
				)
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

	/// Processes a welcome message received via push notification and returns the conversation.
	///
	/// When a user is added to a new group or DM, they receive a welcome message.
	/// This method decrypts and processes that message, creating the conversation
	/// locally if it does not already exist.
	///
	/// - Parameter envelopeBytes: The raw bytes of the welcome message envelope.
	/// - Returns: The ``Conversation`` the welcome message belongs to, or `nil` if
	///   the message could not be processed.
	/// - Throws: If decryption or processing fails.
	public func fromWelcome(envelopeBytes: Data) async throws
		-> Conversation?
	{
		let conversations =
			try await ffiConversations
				.processStreamedWelcomeMessage(envelopeBytes: envelopeBytes)
		guard let firstConversation = conversations.first else {
			return nil
		}
		return try await firstConversation.toConversation(client: client)
	}

	/// Returns the HMAC keys for all conversations, used for push notification decryption.
	///
	/// Push notification services use these keys to decrypt message previews without
	/// having access to the full conversation keys. Each conversation has a set of
	/// rotating HMAC keys indexed by 30-day epochs.
	///
	/// - Returns: A `Xmtp_KeystoreApi_V1_GetConversationHmacKeysResponse` mapping
	///   topic strings to their HMAC key data.
	/// - Throws: If the keys cannot be retrieved from the local database.
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
				Topic.groupMessage(convo.key.toHex).description
			] = hmacKeys
		}

		return hmacKeysResponse
	}

	/// Returns the push notification topic strings for all conversations.
	///
	/// Use these topics to register with a push notification service so the device
	/// receives notifications for incoming messages in every conversation.
	///
	/// - Returns: An array of topic strings, one per conversation.
	/// - Throws: If the local database query fails.
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
