import SwiftUI

struct ConversationTitle: View {
	let title: String
	var body: some View {
		if title.hasPrefix("@") {
			HStack(spacing: 0) { Text("@"); HandleText(handle: title) }
		} else {
			Text(title)
		}
	}
}
