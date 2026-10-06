import Foundation
import XmtpSdk

/// Compile-only checks of public call forms. Nothing calls these functions.
/// The omitted `options` arguments come from the `default(options = None)`
/// annotations on the Rust methods, so the build fails when one is dropped.
enum OmittedOptionsForms {
	static func listenerStop(_ host: SDKClient, _ raw: Client, _ id: ListenerId) async {
		await host.stopListener(id)
		await raw.stopListener(id: id)
	}

	static func sends(
		_ group: Group, _ conversations: Conversations, _ id: MessageId,
		_ reaction: Reaction, _ encoded: EncodedContent,
	) async throws {
		_ = try await group.send(encoded: encoded)
		_ = try await group.prepareMessage(encoded: encoded)
		_ = try await conversations.reactToMessage(id: id, reaction: reaction)
		_ = try await conversations.replyToMessage(id: id, content: encoded)
	}

	static func typedSends(
		_ group: Group, _ id: MessageId, _ reaction: Reaction, _ encoded: EncodedContent,
		_ attachment: Attachment, _ remote: RemoteAttachment, _ multiRemote: MultiRemoteAttachment,
		_ transaction: TransactionReference, _ walletCalls: WalletSendCalls,
		_ actions: Actions, _ intent: Intent,
	) async throws {
		_ = try await group.sendText(text: "text")
		_ = try await group.sendMarkdown(markdown: "markdown")
		_ = try await group.sendReaction(reference: id, referenceInboxId: nil, reaction: reaction)
		_ = try await group.sendReply(reference: id, referenceInboxId: nil, content: encoded)
		_ = try await group.sendReadReceipt()
		_ = try await group.sendAttachment(attachment: attachment)
		_ = try await group.sendRemoteAttachment(attachment: remote)
		_ = try await group.sendMultiRemoteAttachment(attachment: multiRemote)
		_ = try await group.sendTransactionReference(reference: transaction)
		_ = try await group.sendWalletSendCalls(calls: walletCalls)
		_ = try await group.sendActions(actions: actions)
		_ = try await group.sendIntent(intent: intent)
	}

	/// A `Conversation` narrows to exactly a group or a DM.
	static func narrow(_ conversation: Conversation) -> ConversationId {
		switch conversation {
		case let .group(group): group.id()
		case let .dm(dm): dm.id()
		}
	}
}
