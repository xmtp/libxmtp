import LibXMTP

public struct ConversationDebugInfo {
	let ffiConversationDebugInfo: FfiConversationDebugInfo
	
	public init(ffiConversationDebugInfo: FfiConversationDebugInfo) {
		self.ffiConversationDebugInfo = ffiConversationDebugInfo
	}

	public var epoch: UInt64 {
		ffiConversationDebugInfo.epoch
	}

	public var maybeForked: Bool {
		ffiConversationDebugInfo.maybeForked
	}

	public var forkDetails: String {
		ffiConversationDebugInfo.forkDetails
	}
}
