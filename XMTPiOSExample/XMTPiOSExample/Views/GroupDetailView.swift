//
//  GroupDetailView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import SwiftUI
import XMTPiOS

struct GroupDetailView: View {
	var client: Client
	var group: XMTPiOS.Group

	@State private var messages: [Message] = []
	@State private var isShowingSettings = false

	var body: some View {
		VStack {
			MessageListView(myAddress: client.address, messages: messages, isGroup: true)
				.refreshable {
					await loadMessages()
				}
				.task {
					await loadMessages()
				}
				.task {
					do {
						for try await _ in group.streamMessages() {
							await loadMessages()
						}
					} catch {
						print("Erorr streaming group messages \(error)")
					}
				}

			MessageComposerView { text in
				do {
					try await group.send(content: text)
				} catch {
					print("Error sending message: \(error)")
				}
			}
		}
		.navigationTitle("Group Chat")
		.navigationBarTitleDisplayMode(.inline)
		.toolbar {
			Button(action: { isShowingSettings.toggle() }) {
				Label("Settings", systemImage: "gearshape")
			}
			.sheet(isPresented: $isShowingSettings) {
				GroupSettingsView(client: client, group: group)
			}
		}
	}

	func loadMessages() async {
		do {
			try await group.sync()
			let messages = try await group.messages()
			await MainActor.run {
				self.messages = messages
			}
		} catch {
			print("Error loading messages for \(group.id)")
		}
	}
}
