import SwiftUI
import XMTPiOS

struct GroupSettingsView: View {
	var client: XMTPiOS.Client
	var group: XMTPiOS.Group

	@Environment(\.dismiss) var dismiss
	@EnvironmentObject var coordinator: EnvironmentCoordinator

	@State private var groupMembers: [String] = []
	@State private var newGroupMember = ""
	@State private var isAddingMember = false
	@State private var groupError = ""

	init(client: Client, group: XMTPiOS.Group) {
		self.client = client
		self.group = group
	}

	var body: some View {
		NavigationStack {
			List {
				membersSectionView

				if !groupError.isEmpty {
					Text(groupError)
						.foregroundStyle(.red)
						.font(.subheadline)
				}
			}
			.navigationTitle("Group Settings")
			.task {
				try? await syncGroupMembers()
			}
		}
	}

	private var membersSectionView: some View {
		Section("Members") {
			ForEach(groupMembers, id: \.self) { member in
				memberRowView(for: member)
			}

			addMemberView
		}
	}

	private func memberRowView(for member: String) -> some View {
		HStack {
			Text(Util.abbreviate(address: member))
			Spacer()
			if client.publicIdentity.identifier.lowercased() == member.lowercased() {
				Text("You")
					.foregroundStyle(.secondary)
			}
		}
		.swipeActions {
			if client.publicIdentity.identifier.lowercased() == member.lowercased() {
				Button("Leave", role: .destructive) {
					Task {
						try? await leaveGroup()
					}
				}
			} else {
				Button("Remove", role: .destructive) {
					Task {
						try await group.removeMembersByIdentity(identities: [PublicIdentity(kind: .ethereum, identifier: member)])
						try await syncGroupMembers()
					}
				}
			}
		}
	}

	private var addMemberView: some View {
		HStack {
			TextField("Add member", text: $newGroupMember)
			Button("Add") {
				Task {
					await addMember()
				}
			}
			.opacity(isAddingMember ? 0 : 1)
			.overlay {
				if isAddingMember {
					ProgressView()
				}
			}
		}
	}

	private func syncGroupMembers() async throws {
		try await group.sync()
		let inboxIds = try await group.members.map(\.inboxId)
		await MainActor.run {
			groupMembers = inboxIds
		}
	}

	private func leaveGroup() async throws {
		try await group.removeMembersByIdentity(identities: [PublicIdentity(kind: .ethereum, identifier: newGroupMember)])
		await MainActor.run {
			coordinator.path = NavigationPath()
			dismiss()
		}
	}

	private func addMember() async {
		guard newGroupMember.lowercased() != client.publicIdentity.identifier.lowercased() else {
			groupError = "You cannot add yourself to a group"
			return
		}

		isAddingMember = true
		do {
			if try await client.canMessage(identity: PublicIdentity(kind: .ethereum, identifier: newGroupMember)) {
				try await group.addMembersByIdentity(identities: [PublicIdentity(kind: .ethereum, identifier: newGroupMember)])
				try await syncGroupMembers()
				await MainActor.run {
					groupError = ""
					newGroupMember = ""
					isAddingMember = false
				}
			} else {
				await MainActor.run {
					groupError = "Member address not registered"
					isAddingMember = false
				}
			}
		} catch {
			await MainActor.run {
				groupError = error.localizedDescription
				isAddingMember = false
			}
		}
	}
}
