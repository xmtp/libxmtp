import ExampleCore
import SwiftUI
import UIKit

extension ChatContentView {
	@ViewBuilder func message(_ row: ChatMessage) -> some View {
		let mine = row.sender == own
		let attachment = if case .attachment = row.content {
			true
		} else {
			false
		}
		HStack(alignment: .bottom, spacing: 6) {
			if mine {
				Spacer(minLength: 42)
			} else if timeline.conversation?.kind == .group {
				senderAvatar(row)
			}
			VStack(alignment: mine ? .trailing : .leading, spacing: 4) {
				if !mine, !attachment, timeline.conversation?.kind == .group {
					IdentityText(inboxID: row.sender, handle: graph.identityDirectory?.identities[row.sender]?.handle,
					             short: true).font(.caption2).foregroundStyle(.secondary)
				}
				messageBubble(row, mine: mine)
				if mine, !attachment || row.delivery != .published {
					Text(row.delivery == .published ? "Sent" : row.delivery == .failed ? "Not sent · Tap to retry or edit" : "Sending")
						.font(.caption2).foregroundStyle(.secondary)
				}
			}
			if !mine {
				Spacer(minLength: 42)
			}
		}.padding(.vertical, row.reactions.isEmpty ? 0 : 6)
	}

	func senderAvatar(_ row: ChatMessage) -> some View {
		let handle = graph.identityDirectory?.identities[row.sender]?.handle
		return Avatar(seed: row.sender, label: DisplayInitials.person(handle ?? row.sender, isInboxID: handle == nil),
		              size: 24, monospaced: handle == nil)
	}

	func messageBubble(_ row: ChatMessage, mine: Bool) -> some View {
		Group {
			if case .attachment = row.content {
				messageContent(row)
			} else {
				Bubble(isOutgoing: mine) { messageContent(row) }
					.contextMenu { menu(row) } preview: { messagePreview(row) }
			}
		}
		.overlay(alignment: .topTrailing) { reactionBadges(row) }
		.anchorPreference(key: MessageFlightAnchors.self, value: .bounds) { flight?.id == row.id ? [row.id: $0] : [:] }
		.opacity(flight?.id == row.id ? 0 : 1)
		.simultaneousGesture(replyGesture(row))
		.accessibilityActions { messageAccessibilityActions(row, mine: mine) }
	}

	func reactionBadges(_ row: ChatMessage) -> some View {
		HStack(spacing: 2) {
			ForEach(MessageMapper.reactions(row.reactions, own: own)) { reaction in
				Button { graph.navigate(.reactions(conversationID: id, messageID: row.id, emoji: reaction.emoji)) } label: {
					ReactionBadge(emoji: reaction.emoji, count: reaction.count, isSelected: reaction.mine)
				}.buttonStyle(.plain).frame(minWidth: 44, minHeight: 44).contentShape(Rectangle())
			}
		}.offset(y: -12)
	}

	func messagePreview(_ row: ChatMessage) -> some View {
		Text(row.content.text).padding()
	}

	func replyGesture(_ row: ChatMessage) -> some Gesture {
		DragGesture(minimumDistance: 12).onChanged { value in
			let armed = allowed && !row.content.isDeleted && abs(value.translation.width) >= 48
				&& abs(value.translation.width) > abs(value.translation.height)
			if armed, armedReplyID != row.id {
				armedReplyID = row.id; UIImpactFeedbackGenerator(style: .light).impactOccurred()
			}
			if !armed, armedReplyID == row.id {
				armedReplyID = nil
			}
		}.onEnded { _ in
			if armedReplyID == row.id {
				runtime.composer.send(.reply(row.id))
			}
			armedReplyID = nil
		}
	}

	@ViewBuilder func messageAccessibilityActions(_ row: ChatMessage, mine: Bool) -> some View {
		if allowed, !row.content.isDeleted {
			Button("Reply") { runtime.composer.send(.reply(row.id)) }
			Button("React") { graph.navigate(.emojiPicker(conversationID: id, messageID: row.id)) }
		}
		Button("Copy") { UIPasteboard.general.string = row.content.text }
		if allowed, mine, row.delivery == .published {
			Button("Delete") { runtime.actions.send(.requestDelete(row)) }
		}
	}

	@ViewBuilder func messageContent(_ row: ChatMessage) -> some View {
		switch row.content {
		case let .attachment(file): AttachmentCardView(
				id: row.id,
				descriptor: file,
				store: runtime.attachments,
				graph: graph,
				allowed: allowed,
				own: row.sender == own,
				sender: row.sender,
				sentAt: row.date,
				messageActions: AnyView(menu(row)),
			)
		case let .reply(quote, body): VStack(alignment: .leading, spacing: 6) {
				Text(quote).font(.caption).lineLimit(3).padding(8).background(
					.black.opacity(0.08),
					in: RoundedRectangle(cornerRadius: 8),
				); Text(body)
			}
		case .deleted: Text("Message deleted", tableName: "Chat").italic()
		default: Text(row.content.text)
		}
	}

	@ViewBuilder func menu(_ row: ChatMessage) -> some View {
		if allowed, !row.content.isDeleted {
			ControlGroup {
				ForEach(["👍", "❤️", "😂", "😮", "😢"], id: \.self) { emoji in
					Button(emoji) { runtime.actions.send(.react(row, emoji)) }
				}
				Button("More", systemImage: "plus") {
					graph.navigate(.emojiPicker(conversationID: id, messageID: row.id))
				}.labelStyle(.iconOnly)
			}.controlGroupStyle(.palette).menuActionDismissBehavior(.enabled)
		}

		Button(String(localized: "Copy", table: "Chat"), systemImage: "doc.on.doc") {
			UIPasteboard.general.string = row.content.text
		}
		if allowed,
		   !row.content
		   .isDeleted
		{
			Button(String(localized: "Reply", table: "Chat"), systemImage: "arrowshape.turn.up.left") {
				runtime.composer.send(.reply(row.id))
			}; Button(
				"React",
				systemImage: "face.smiling",
			) { graph.navigate(.emojiPicker(conversationID: id, messageID: row.id)) }
		}
		if allowed, row.sender == own {
			if row.delivery == .published {
				Button(
					"Delete",
					systemImage: "trash",
					role: .destructive,
				) { runtime.actions.send(.requestDelete(row)) }
			} else {
				Button(String(localized: "Retry", table: "Chat")) { runtime.actions.send(.retry(row)) }; Button(String(
					localized: "Edit",
					table: "Chat",
				)) {
					runtime.composer.send(.edit(row.content.text))
				}
			}
		}
	}
}
