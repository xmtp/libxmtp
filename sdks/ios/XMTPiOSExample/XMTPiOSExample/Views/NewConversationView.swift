import SwiftUI
import XmtpSdk

struct NewConversationView: View {
	var client: SDKClient
	var onCreate: (Conversation) -> Void
	@Environment(\.dismiss) private var dismiss
	@State private var address = ""
	@State private var members: [PublicIdentity] = []
	@State private var error: String?
	@State private var isCreating = false

	var body: some View {
		Form {
			TextField("Ethereum address", text: $address).textInputAutocapitalization(.never)
			Button("Create DM") { create(isGroup: false) }.disabled(address.isEmpty)
			Section("Group members") {
				ForEach(members, id: \.identifier) { Text($0.identifier) }
				Button("Add address") {
					members.append(PublicIdentity(identifier: address, kind: .ethereum))
					address = ""
				}.disabled(address.isEmpty)
				Button("Create group") { create(isGroup: true) }.disabled(members.isEmpty)
			}
			if let error {
				Text(error).foregroundStyle(.red)
			}
		}.disabled(isCreating).navigationTitle("New conversation")
	}

	private func create(isGroup: Bool) {
		Task {
			isCreating = true
			defer { isCreating = false }
			do {
				let conversation: Conversation = if isGroup {
					try await .group(group: client.conversations().createGroup(members: members, options: nil))
				} else {
					try await .dm(dm: client.conversations().createDm(
						peer: PublicIdentity(identifier: address, kind: .ethereum), options: nil,
					))
				}
				onCreate(conversation)
				dismiss()
			} catch { self.error = error.localizedDescription }
		}
	}
}
