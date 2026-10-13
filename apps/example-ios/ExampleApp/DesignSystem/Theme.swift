import SwiftUI

/// Brand and high-contrast secondary labels are asset-backed; other surfaces use system colors.
enum Theme {
	static let tint = Color("AccentColor")
	static let accentText = Color("AccentText")
	static let secondaryLabel = Color("SecondaryLabel")
	static let incomingBubble = Color("IncomingBubble")
	static let minimumHitTarget: CGFloat = 44
}

enum Symbol {
	static let back = "chevron.left"
	static let settings = "gearshape"
	static let compose = "square.and.pencil"
	static let search = "magnifyingglass"
	static let add = "plus"
	static let close = "xmark"
	static let send = "arrow.up"
	static let down = "arrow.down"
	static let requests = "tray"
	static let notifications = "bell"
	static let muted = "bell.slash"
	static let reply = "arrowshape.turn.up.left"
	static let copy = "doc.on.doc"
	static let delete = "trash"
	static let retry = "arrow.clockwise"
	static let photo = "photo"
	static let file = "doc"
	static let download = "arrow.down.circle"
	static let share = "square.and.arrow.up"
	static let save = "square.and.arrow.down"
	static let addMember = "person.badge.plus"
	static let disappearing = "hourglass"
	static let lock = "lock"
	static let key = "key"
	static let verified = "checkmark.seal"
	static let error = "exclamationmark.circle"
	static let block = "nosign"
	static let signOut = "rectangle.portrait.and.arrow.right"
	static let developer = "chevron.left.forwardslash.chevron.right"
	static let permissions = "slider.horizontal.3"
	static let reaction = "face.smiling"
	static let conversations = "bubble.left.and.bubble.right"
	static let handle = "at"
}
