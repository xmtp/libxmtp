import SwiftData
import SwiftUI
import XMTPiOS

// Display the user's profile info.
struct UserView: View {
	@Environment(XmtpSession.self) private var session
	let inboxId: String

	var body: some View {
		//        Text("TODO: User Profile \(user.first?.identifiers.first?.serialized ?? "Unknown")")
		Text("TODO: User Profile \(inboxId)")
			.lineLimit(1)
	}
}
