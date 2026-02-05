import Foundation

/// Content type ID for deleted messages (used in enriched messages).
public let ContentTypeDeletedMessage = ContentTypeID(
	authorityID: "xmtp.org",
	typeID: "deletedMessage",
	versionMajor: 1,
	versionMinor: 0,
)

/// Represents a message that has been deleted.
/// This is used in enriched messages to indicate that a message was deleted.
public struct DeletedMessage: Codable, Equatable {
	public let deletedBy: DeletedBy

	public init(deletedBy: DeletedBy) {
		self.deletedBy = deletedBy
	}
}

/// Indicates who deleted the message.
public enum DeletedBy: Codable, Equatable {
	/// The original sender deleted their own message
	case sender
	/// An admin deleted the message
	case admin(inboxId: String)
}
