import Foundation

extension FfiConversation {
	func groupFromFFI(client: Client) -> Group {
		Group(ffiGroup: self, client: client)
	}

	func dmFromFFI(client: Client) -> Dm {
		Dm(ffiConversation: self, client: client)
	}

	func toConversation(client: Client) async throws -> Conversation {
		if conversationType() == .dm {
			Conversation.dm(dmFromFFI(client: client))
		} else {
			Conversation.group(groupFromFFI(client: client))
		}
	}
}

extension FfiConversationListItem {
	func groupFromFFI(client: Client) -> Group {
		Group(
			ffiGroup: conversation(), ffiLastMessage: lastMessage(),
			ffiCommitLogForkStatus: isCommitLogForked(), client: client,
		)
	}

	func dmFromFFI(client: Client) -> Dm {
		Dm(
			ffiConversation: conversation(), ffiLastMessage: lastMessage(),
			ffiCommitLogForkStatus: isCommitLogForked(), client: client,
		)
	}

	func toConversation(client: Client) async throws -> Conversation {
		if conversation().conversationType() == .dm {
			Conversation.dm(dmFromFFI(client: client))
		} else {
			Conversation.group(groupFromFFI(client: client))
		}
	}
}

extension FfiConversationMember {
	var fromFFI: Member {
		Member(ffiGroupMember: self)
	}
}

extension [ConsentState] {
	var toFFI: [FfiConsentState] {
		map(\.toFFI)
	}
}

extension ConsentState {
	var toFFI: FfiConsentState {
		switch self {
		case .allowed: FfiConsentState.allowed
		case .denied: FfiConsentState.denied
		default: FfiConsentState.unknown
		}
	}
}

extension FfiConsentState {
	var fromFFI: ConsentState {
		switch self {
		case .allowed: ConsentState.allowed
		case .denied: ConsentState.denied
		default: ConsentState.unknown
		}
	}
}

extension FfiConsentEntityType {
	var fromFFI: EntryType {
		switch self {
		case .inboxId: EntryType.inbox_id
		case .conversationId: EntryType.conversation_id
		}
	}
}

extension EntryType {
	var toFFI: FfiConsentEntityType {
		switch self {
		case .conversation_id: FfiConsentEntityType.conversationId
		case .inbox_id: FfiConsentEntityType.inboxId
		}
	}
}

extension ConsentRecord {
	var toFFI: FfiConsent {
		FfiConsent(
			entityType: entryType.toFFI, state: consentType.toFFI, entity: value,
		)
	}
}

extension FfiConsent {
	var fromFfi: ConsentRecord {
		ConsentRecord(
			value: entity, entryType: entityType.fromFFI,
			consentType: state.fromFFI,
		)
	}
}

extension FfiGroupMembershipState {
	var fromFFI: GroupMembershipState {
		switch self {
		case .allowed: .allowed
		case .rejected: .rejected
		case .pending: .pending
		case .restored: .restored
		case .pendingRemove: .pendingRemove
		}
	}
}
