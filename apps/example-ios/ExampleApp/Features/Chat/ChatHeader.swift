import SwiftUI

struct ChatHeader: View {
	let id: String
	let title: String
	let initials: String
	var memberCount: Int?
	var body: some View {
		VStack(spacing: 2) {
			Avatar(seed: id, label: initials, size: 30)
			HStack(spacing: 4) {
				ConversationTitle(title: title).font(.caption.weight(.semibold)).lineLimit(1)
				if let memberCount {
					Text("· \(memberCount) members").font(.caption).foregroundStyle(Theme.secondaryLabel)
						.accessibilityIdentifier("chat.member-count")
				}
				Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(Theme.secondaryLabel)
			}
		}.accessibilityElement(children: .combine)
	}
}
