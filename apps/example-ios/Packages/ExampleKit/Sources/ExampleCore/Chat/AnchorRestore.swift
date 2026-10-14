import Foundation

public struct ScrollAnchor: Codable, Equatable, Sendable {
	public var messageID: String
	public var position: HistoryPosition
	public init(messageID: String, position: HistoryPosition) {
		self.messageID = messageID
		self.position = position
	}
}

public enum AnchorRestore {
	public static func target(_ anchor: ScrollAnchor, rows: [ChatMessage]) -> (id: String?, changed: Bool) {
		if rows.contains(where: { $0.id == anchor.messageID && !$0.content.isDeleted }) {
			return (anchor.messageID, false)
		}
		if let index = rows.firstIndex(where: { $0.id == anchor.messageID }) {
			let newer = rows[..<index].reversed().first { !$0.content.isDeleted }
			let older = rows.dropFirst(index + 1)
				.first { !$0.content.isDeleted }
			return (
				newer?.id ?? older?.id,
				true,
			)
		}
		let newer = rows.filter { !$0.content.isDeleted && $0.sentAt >= anchor.position.sentAt }.min { $0.sentAt < $1.sentAt }
		let older = rows.filter { !$0.content.isDeleted && $0.sentAt < anchor.position.sentAt }.max { $0.sentAt < $1.sentAt }
		return (newer?.id ?? older?.id, true)
	}
}
