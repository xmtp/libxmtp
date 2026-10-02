import SwiftUI

struct CreateConversationView: View {
	@Environment(XmtpSession.self) private var session
	@Environment(Router.self) private var router
	@State private var peer = ""
	@State private var isGroup = false
	@State private var error: String?
	@State private var isCreating = false

	var body: some View {
		Form {
			TextField("Peer inbox ID", text: $peer)
				.textInputAutocapitalization(.never)
			Toggle("Create a group", isOn: $isGroup)
			if let error {
				Text(error).foregroundStyle(.red)
			}
			Button("Create") {
				Task {
					isCreating = true
					defer { isCreating = false }
					do {
						let id = try await session.createConversation(peer: peer, isGroup: isGroup)
						router.push(route: .conversation(conversationId: id))
					} catch { self.error = error.localizedDescription }
				}
			}.disabled(peer.isEmpty || isCreating)
		}.navigationTitle("New conversation")
	}
}
