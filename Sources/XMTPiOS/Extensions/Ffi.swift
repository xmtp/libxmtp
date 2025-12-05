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
			return Conversation.dm(dmFromFFI(client: client))
		} else {
			return Conversation.group(groupFromFFI(client: client))
		}
	}
}

extension FfiConversationListItem {
	func groupFromFFI(client: Client) -> Group {
		Group(
			ffiGroup: conversation(), ffiLastMessage: lastMessage(),
			ffiCommitLogForkStatus: isCommitLogForked(), client: client
		)
	}

	func dmFromFFI(client: Client) -> Dm {
		Dm(
			ffiConversation: conversation(), ffiLastMessage: lastMessage(),
			ffiCommitLogForkStatus: isCommitLogForked(), client: client
		)
	}

	func toConversation(client: Client) async throws -> Conversation {
		if conversation().conversationType() == .dm {
			return Conversation.dm(dmFromFFI(client: client))
		} else {
			return Conversation.group(groupFromFFI(client: client))
		}
	}
}

extension FfiConversationMember {
	var fromFFI: Member {
		Member(ffiGroupMember: self)
	}
}

extension Array where Element == ConsentState {
	var toFFI: [FfiConsentState] {
		map(\.toFFI)
	}
}

extension ConsentState {
	var toFFI: FfiConsentState {
		switch self {
		case .allowed: return FfiConsentState.allowed
		case .denied: return FfiConsentState.denied
		default: return FfiConsentState.unknown
		}
	}
}

extension FfiConsentState {
	var fromFFI: ConsentState {
		switch self {
		case .allowed: return ConsentState.allowed
		case .denied: return ConsentState.denied
		default: return ConsentState.unknown
		}
	}
}

extension FfiConsentEntityType {
	var fromFFI: EntryType {
		switch self {
		case .inboxId: return EntryType.inbox_id
		case .conversationId: return EntryType.conversation_id
		}
	}
}

extension EntryType {
	var toFFI: FfiConsentEntityType {
		switch self {
		case .conversation_id: return FfiConsentEntityType.conversationId
		case .inbox_id: return FfiConsentEntityType.inboxId
		}
	}
}

extension ConsentRecord {
	var toFFI: FfiConsent {
		FfiConsent(
			entityType: entryType.toFFI, state: consentType.toFFI, entity: value
		)
	}
}

extension FfiConsent {
	var fromFfi: ConsentRecord {
		ConsentRecord(
			value: entity, entryType: entityType.fromFFI,
			consentType: state.fromFFI
		)
	}
}

extension FfiGroupMembershipState {
	var fromFFI: GroupMembershipState {
		switch self {
		case .allowed: return .allowed
		case .rejected: return .rejected
		case .pending: return .pending
		case .restored: return .restored
		case .pendingRemove: return .pendingRemove
		}
	}
}
