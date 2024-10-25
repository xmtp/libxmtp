//
//  File.swift
//  
//
//  Created by Pat Nakajima on 1/16/24.
//

import Foundation
import LibXMTP

// MARK: PagingInfo

extension PagingInfo {
	var toFFI: FfiPagingInfo {
		FfiPagingInfo(limit: limit, cursor: cursor.toFFI, direction: direction.toFFI)
	}
}

extension FfiPagingInfo {
	var fromFFI: PagingInfo {
		PagingInfo.with {
			$0.limit = limit

			if let cursor {
				$0.cursor = cursor.fromFFI
			}

			$0.direction = direction.fromFFI
		}
	}
}

extension Cursor {
	var toFFI: FfiCursor {
		FfiCursor(digest: self.index.digest, senderTimeNs: self.index.senderTimeNs)
	}
}

extension FfiCursor {
	var fromFFI: Cursor {
		Cursor.with {
			$0.index.digest = Data(digest)
			$0.index.senderTimeNs = senderTimeNs
		}
	}
}

extension PagingInfoSortDirection {
	var toFFI: FfiSortDirection {
		switch self {
		case .ascending:
			return .ascending
		case .descending:
			return .descending
		default:
			return .unspecified
		}
	}
}

extension FfiSortDirection {
	var fromFFI: PagingInfoSortDirection {
		switch self {
		case .ascending:
			return .ascending
		case .descending:
			return .descending
		default:
			return .unspecified
		}
	}
}

// MARK: QueryRequest

extension QueryRequest {
	var toFFI: FfiV2QueryRequest {
		FfiV2QueryRequest(
			contentTopics: contentTopics,
			startTimeNs: startTimeNs,
			endTimeNs: endTimeNs,
			pagingInfo: pagingInfo.toFFI
		)
	}
}

extension FfiV2QueryRequest {
	var fromFFI: QueryRequest {
		QueryRequest.with {
			$0.contentTopics = contentTopics
			$0.startTimeNs = startTimeNs
			$0.endTimeNs = endTimeNs
			$0.pagingInfo = pagingInfo?.fromFFI ?? PagingInfo()
		}
	}
}

// MARK: BatchQueryRequest

extension BatchQueryRequest {
	var toFFI: FfiV2BatchQueryRequest {
		FfiV2BatchQueryRequest(requests: requests.map(\.toFFI))
	}
}

extension FfiV2BatchQueryRequest {
	var fromFFI: BatchQueryRequest {
		BatchQueryRequest.with {
			$0.requests = requests.map(\.fromFFI)
		}
	}
}

// MARK: QueryResponse

extension QueryResponse {
	var toFFI: FfiV2QueryResponse {
		FfiV2QueryResponse(envelopes: envelopes.map(\.toFFI), pagingInfo: nil)
	}
}

extension FfiV2QueryResponse {
	var fromFFI: QueryResponse {
		QueryResponse.with {
			$0.envelopes = envelopes.map(\.fromFFI)
			$0.pagingInfo = pagingInfo?.fromFFI ?? PagingInfo()
		}
	}
}

// MARK: BatchQueryResponse

extension BatchQueryResponse {
	var toFFI: FfiV2BatchQueryResponse {
		FfiV2BatchQueryResponse(responses: responses.map(\.toFFI))
	}
}

extension FfiV2BatchQueryResponse {
	var fromFFI: BatchQueryResponse {
		BatchQueryResponse.with {
			$0.responses = responses.map(\.fromFFI)
		}
	}
}

// MARK: Envelope

extension Envelope {
	var toFFI: FfiEnvelope {
		FfiEnvelope(contentTopic: contentTopic, timestampNs: timestampNs, message: message)
	}
}

extension FfiEnvelope {
	var fromFFI: Envelope {
		Envelope.with {
			$0.contentTopic = contentTopic
			$0.timestampNs = timestampNs
			$0.message = Data(message)
		}
	}
}

// MARK: PublishRequest

extension PublishRequest {
	var toFFI: FfiPublishRequest {
		FfiPublishRequest(envelopes: envelopes.map(\.toFFI))
	}
}

extension FfiPublishRequest {
	var fromFFI: PublishRequest {
		PublishRequest.with {
			$0.envelopes = envelopes.map(\.fromFFI)
		}
	}
}

// MARK: SubscribeRequest
extension SubscribeRequest {
	var toFFI: FfiV2SubscribeRequest {
		FfiV2SubscribeRequest(contentTopics: contentTopics)
	}
}

extension FfiV2SubscribeRequest {
	var fromFFI: SubscribeRequest {
		SubscribeRequest.with {
			$0.contentTopics = contentTopics
		}
	}
}

// MARK: Group

extension FfiConversation {
	func groupFromFFI(client: Client) -> Group {
		Group(ffiGroup: self, client: client)
	}
	
	func dmFromFFI(client: Client) -> Dm {
		Dm(ffiConversation: self, client: client)
	}
	
	func toConversation(client: Client) throws -> Conversation {
		if (try groupMetadata().conversationType() == "dm") {
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
	var toFFI: FfiConsentState{
		switch (self) {
		case .allowed: return FfiConsentState.allowed
		case .denied: return FfiConsentState.denied
		default: return FfiConsentState.unknown
		}
	}
}

extension FfiConsentState {
	var fromFFI: ConsentState{
		switch (self) {
		case .allowed: return ConsentState.allowed
		case .denied: return ConsentState.denied
		default: return ConsentState.unknown
		}
	}
}

extension EntryType {
	var toFFI: FfiConsentEntityType{
		switch (self) {
		case .group_id: return FfiConsentEntityType.conversationId
		case .inbox_id: return FfiConsentEntityType.inboxId
		case .address: return FfiConsentEntityType.address
		}
	}
}

extension ConsentListEntry {
	var toFFI: FfiConsent {
		FfiConsent(entityType: entryType.toFFI, state: consentType.toFFI, entity: value)
	}
}
