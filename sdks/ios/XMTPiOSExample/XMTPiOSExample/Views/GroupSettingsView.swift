import SwiftUI
import XmtpSdk

struct GroupSettingsView: View {
	var client: SDKClient
	var group: XmtpSdk.Group
	@Environment(\.dismiss) private var dismiss
	@EnvironmentObject private var coordinator: EnvironmentCoordinator
	@State private var members: [Member] = []
	@State private var inboxId = ""
	@State private var error: String?
	@State private var isUpdating = false

	var body: some View {
		NavigationStack {
			List {
				Section("Members") {
					ForEach(members, id: \.inboxId) { member in
						Text(member.inboxId).swipeActions {
							Button(member.inboxId == client.inboxId() ? "Leave" : "Remove", role: .destructive) {
								update(remove: member.inboxId)
							}
						}
					}
					TextField("Member inbox ID", text: $inboxId).textInputAutocapitalization(.never)
					Button("Add member") { update(remove: nil) }.disabled(inboxId.isEmpty)
				}
				if let error {
					Text(error).foregroundStyle(.red)
				}
			}.disabled(isUpdating).navigationTitle("Group settings")
				.task { await reload() }
		}
	}

	private func reload() async {
		do {
			try await group.sync()
			members = try await group.members()
		} catch { self.error = error.localizedDescription }
	}

	private func update(remove: String?) {
		Task {
			isUpdating = true
			defer { isUpdating = false }
			do {
				if let remove {
					if remove == client.inboxId() {
						try await group.requestRemoval()
						coordinator.path = NavigationPath(); dismiss(); return
					}
					try await group.removeMembers(members: [remove])
				} else {
					_ = try await group.addMembers(members: [inboxId])
					inboxId = ""
				}
				await reload()
			} catch { self.error = error.localizedDescription }
		}
	}
}
