//
//  GroupSettingsView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 2/6/24.
//

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
				Section("Members") {
					ForEach(groupMembers, id: \.self) { member in
						HStack {
							Text(Util.abbreviate(address: member))
							Spacer()
							if client.address.lowercased() == member.lowercased() {
								Text("You")
									.foregroundStyle(.secondary)
							}
						}
						.swipeActions {
							if client.address.lowercased() == member.lowercased() {
								Button("Leave", role: .destructive) {
									Task {
										try await group.removeMembers(addresses: [client.address])
										coordinator.path = NavigationPath()
										dismiss()
									}
								}
							} else {
								Button("Remove", role: .destructive) {
									Task {
										try await group.removeMembers(addresses: [member])
										await syncGroupMembers()
									}
								}
							}
						}
					}

					HStack {
						TextField("Add member", text: $newGroupMember)
						Button("Add") {
							if newGroupMember.lowercased() == client.address {
								self.groupError = "You cannot add yourself to a group"
								return
							}

							isAddingMember = true

							Task {
								do {
									if try await self.client.canMessageV3(address: newGroupMember) {
										try await group.addMembers(addresses: [newGroupMember])
										try await syncGroupMembers()

										await MainActor.run {
											self.groupError = ""
											self.newGroupMember = ""
											self.isAddingMember = false
										}
									} else {
										await MainActor.run {
											self.groupError = "Member address not registered"
											self.isAddingMember = false
										}
									}
								} catch {
									self.groupError = error.localizedDescription
									self.isAddingMember = false
								}
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

				if groupError != "" {
					Text(groupError)
						.foregroundStyle(.red)
						.font(.subheadline)
				}
			}
			.navigationTitle("Group Settings")
			.task {
				await syncGroupMembers()
			}
		}
	}

	private func syncGroupMembers() async {
		// swiftlint:disable no_optional_try
		try? await group.sync()
		// swiftlint:enable no_optional_try
		await MainActor.run {
			self.groupMembers = group.memberAddresses
		}
	}
}
