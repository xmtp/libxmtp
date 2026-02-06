//
//  NewConversationView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/7/22.
//

import SwiftUI
import XMTPiOS

struct NewConversationView: View {
	var client: XMTPiOS.Client
	var onCreate: (XMTPiOS.Conversation) -> Void

	@Environment(\.dismiss) var dismiss
	@State private var recipientAddress = ""
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

			Section(header: Text("Or Create a Group")) {
				ForEach(groupMembers, id: \.self) { member in
					Text(member)
				}

				HStack {
					TextField("Add member", text: $newGroupMember)
					Button("Add") {
						if newGroupMember.lowercased() == client.publicIdentity.identifier.lowercased() {
							groupError = "You cannot add yourself to a group"
							return
						}

						isAddingMember = true

						Task {
							do {
								if try await client.canMessage(identity: PublicIdentity(kind: .ethereum, identifier: newGroupMember)) {
									await MainActor.run {
										groupError = ""
										groupMembers.append(newGroupMember)
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
								groupError = error.localizedDescription
								isAddingMember = false
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
							let identities = groupMembers.map { PublicIdentity(kind: .ethereum, identifier: $0) }
							let group = try await client.conversations.newGroupWithIdentities(with: identities)
							try await client.conversations.sync()
							await MainActor.run {
								dismiss()
								onCreate(.group(group))
							}
						} catch {
							await MainActor.run {
								groupError = error.localizedDescription
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
				let conversation = try await client.conversations.newConversationWithIdentity(with: PublicIdentity(
					kind: .ethereum,
					identifier: address,
				))
				await MainActor.run {
					dismiss()
					onCreate(conversation)
				}
			} catch ConversationError.memberNotRegistered([address]) {
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
