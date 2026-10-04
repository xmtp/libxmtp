//
//  GroupDetailView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import SwiftUI
import XmtpSdk

struct GroupDetailView: View {
	var client: SDKClient
	var group: XmtpSdk.Group

	@State private var messages: [Message] = []
	@State private var isShowingSettings = false

	var body: some View {
		VStack {
			MessageListView(myAddress: client.inboxId(), messages: messages, isGroup: true)
				.refreshable {
					await loadMessages()
				}
				.task {
					await loadMessages()
				}
				.task {
					do {
						for try await _ in try await client.messages(in: group) {
							await loadMessages()
						}
					} catch {
						print("Erorr streaming group messages \(error)")
					}
				}

			MessageComposerView { text in
				do {
					try await group.sendText(text: text, options: nil)
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
			let messages = try await group.messages(options: nil)
			await MainActor.run {
				self.messages = messages
			}
		} catch {
			print("Error loading messages for \(group.id())")
		}
	}
}
