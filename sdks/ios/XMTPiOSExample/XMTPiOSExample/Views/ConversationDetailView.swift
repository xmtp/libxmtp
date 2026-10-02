//
//  ConversationDetailView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 12/2/22.
//

import SwiftUI
import XmtpSdk

struct ConversationDetailView: View {
	var client: SDKClient
	var conversation: Conversation

	@State private var messages: [Message] = []

	var body: some View {
		VStack {
			MessageListView(myAddress: client.inboxId(), messages: messages)
				.refreshable {
					await loadMessages()
				}
				.task {
					await loadMessages()
				}
				.task {
					do {
						let stream: SDKMessageStream = switch conversation {
						case let .group(group): try await client.messages(in: group)
						case let .dm(dm): try await client.messages(in: dm)
						}
						for try await message in stream {
							await MainActor.run {
								messages.append(message)
							}
						}
					} catch {
						print("Error in message stream: \(error)")
					}
				}

			MessageComposerView { text in
				do {
					try await conversation.sendText(text: text, options: nil)
				} catch {
					print("Error sending message: \(error)")
				}
			}
		}
		.navigationTitle(conversation.id())
		.navigationBarTitleDisplayMode(.inline)
	}

	func loadMessages() async {
		do {
			let messages = try await conversation.messages(options: nil)
			await MainActor.run {
				self.messages = messages
			}
		} catch {
			print("Error loading messages for \(conversation.id())")
		}
	}
}
