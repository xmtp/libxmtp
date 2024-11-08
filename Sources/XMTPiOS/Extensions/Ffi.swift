import Foundation
import LibXMTP

extension FfiConversation {
	func groupFromFFI(client: Client) -> Group {
		Group(ffiGroup: self, client: client)
	}

	func dmFromFFI(client: Client) -> Dm {
		Dm(ffiConversation: self, client: client)
	}

	func toConversation(client: Client) throws -> Conversation {
		if try groupMetadata().conversationType() == "dm" {
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

extension EntryType {
	var toFFI: FfiConsentEntityType {
		switch self {
		case .conversation_id: return FfiConsentEntityType.conversationId
		case .inbox_id: return FfiConsentEntityType.inboxId
		case .address: return FfiConsentEntityType.address
		}
	}
}

extension ConsentListEntry {
	var toFFI: FfiConsent {
		FfiConsent(
			entityType: entryType.toFFI, state: consentType.toFFI, entity: value
		)
	}
}
