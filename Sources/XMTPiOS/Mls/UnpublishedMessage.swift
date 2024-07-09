//
//  UnpublishedMessage.swift
//  
//
//  Created by Naomi Plasterer on 7/8/24.
//

import Foundation
import LibXMTP

public struct UnpublishedMessage: Identifiable {
	let ffiUnpublishedMessage: FfiUnpublishedMessage
	
	init(ffiUnpublishedMessage: FfiUnpublishedMessage) {
		self.ffiUnpublishedMessage = ffiUnpublishedMessage
	}
	
	public var id: String {
		return ffiUnpublishedMessage.id().toHex
	}
	
	public func publish() async throws -> String {
		try await ffiUnpublishedMessage.publish()
		return id
	}
}
