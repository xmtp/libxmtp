import Foundation
import LibXMTP

extension FfiConversation {
	func groupFromFFI(client: Client) -> Group {
		Group(ffiGroup: self, client: client)
	}

	func dmFromFFI(client: Client) -> Dm {
		Dm(ffiConversation: self, client: client)
	}

	func toConversation(client: Client) async throws -> Conversation {
		if try await conversationType() == .dm {
			return Conversation.dm(self.dmFromFFI(client: client))
		} else {
			return Conversation.group(self.groupFromFFI(client: client))
		}
	}
}

extension FfiConversationListItem {
	func groupFromFFI(client: Client) -> Group {
		Group(
			ffiGroup: self.conversation(), ffiLastMessage: self.lastMessage(),
			client: client)
	}

	func dmFromFFI(client: Client) -> Dm {
		Dm(
			ffiConversation: self.conversation(),
			ffiLastMessage: self.lastMessage(), client: client)
	}

	func toConversation(client: Client) async throws -> Conversation {
		if try await conversation().conversationType() == .dm {
			return Conversation.dm(self.dmFromFFI(client: client))
		} else {
			return Conversation.group(self.groupFromFFI(client: client))
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
		return self.map { $0.toFFI }
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
		case .address: return EntryType.address
		case .conversationId: return EntryType.conversation_id
		}
	}
}

extension EntryType {
	var toFFI: FfiConsentEntityType {
		switch self {
		case .conversation_id: return FfiConsentEntityType.conversationId
		case .inbox_id: return FfiConsentEntityType.inboxId
		case .address: return FfiConsentEntityType.address
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
			value: self.entity, entryType: self.entityType.fromFFI,
			consentType: self.state.fromFFI)
	}
}
