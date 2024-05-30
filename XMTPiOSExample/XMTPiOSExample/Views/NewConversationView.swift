//
//  NewConversationView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/7/22.
//

import SwiftUI
import XMTPiOS

enum ConversationOrGroup: Hashable {
	
	case conversation(Conversation), group(XMTPiOS.Group)

	static func == (lhs: ConversationOrGroup, rhs: ConversationOrGroup) throws -> Bool {
		try lhs.id == rhs.id
	}

	func hash(into hasher: inout Hasher) throws {
		try id.hash(into: &hasher)
	}

	var id: String {
		switch self {
		case .conversation(let conversation):
			return conversation.topic
		case .group(let group):
			return group.id.toHexString()
		}
	}

	var createdAt: Date {
		switch self {
		case .conversation(let conversation):
			return conversation.createdAt
		case .group(let group):
			return group.createdAt
		}
	}
}

struct NewConversationView: View {
	var client: XMTPiOS.Client
	var onCreate: (ConversationOrGroup) -> Void

	@Environment(\.dismiss) var dismiss
	@State private var recipientAddress: String = ""
	@State private var error: String?

	@State private var groupMembers: [String] = []
	@State private var newGroupMember = ""
	@State private var isAddingMember = false
	@State private var groupError = ""

	var body: some View {
		Form {
			Section("Recipient Address") {
				TextField("Enter address here…", text: $recipientAddress)
					.onChange(of: recipientAddress) { newAddress in
						check(address: newAddress)
					}

				if let error {
					Text(error)
						.font(.caption)
						.foregroundColor(.secondary)
				}
			}

			Section("Or Create a Group") {
				ForEach(groupMembers, id: \.self) { member in
					Text(member)
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
									await MainActor.run {
										self.groupError = ""
										self.groupMembers.append(newGroupMember)
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

				if groupError != "" {
					Text(groupError)
						.foregroundStyle(.red)
						.font(.subheadline)
				}

				Button("Create Group") {
					Task {
						do {
							let group = try await client.conversations.newGroup(with: groupMembers)
							try await client.conversations.sync()
							await MainActor.run {
								dismiss()
								onCreate(.group(group))
							}
						} catch {
							await MainActor.run {
								self.groupError = error.localizedDescription
							}
						}
					}
				}
				.disabled(!createGroupEnabled)
			}
			.disabled(isAddingMember)
		}
		.navigationTitle("New conversation")
	}

	var createGroupEnabled: Bool {
		if groupError != "" {
			return false
		}

		if groupMembers.isEmpty {
			return false
		}

		return true
	}

	private func check(address: String) {
		if address.count != 42 {
			return
		}

		error = nil

		Task {
			do {
				let conversation = try await client.conversations.newConversation(with: address)
				await MainActor.run {
					dismiss()
					onCreate(.conversation(conversation))
				}
			} catch ConversationError.recipientNotOnNetwork {
				await MainActor.run {
					self.error = "Recipient is not on the XMTP network."
				}
			} catch {
				await MainActor.run {
					self.error = error.localizedDescription
				}
			}
		}
	}
}

struct NewConversationView_Previews: PreviewProvider {
	static var previews: some View {
		NavigationStack {
			VStack {
				PreviewClientProvider { client in
					Text("Hi")
						.sheet(isPresented: .constant(true)) {
							NewConversationView(client: client) { _ in }
						}
				}
			}
		}
	}
}
