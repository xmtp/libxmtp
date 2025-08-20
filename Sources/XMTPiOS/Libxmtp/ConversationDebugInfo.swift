import LibXMTP

public enum CommitLogForkStatus {
	case forked
	case notForked
	case unknown
}

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
    
    public var localCommitLog: String {
        ffiConversationDebugInfo.localCommitLog
    }
    
    public var remoteCommitLog: String {
        ffiConversationDebugInfo.remoteCommitLog
    }
	
	public var commitLogForkStatus: CommitLogForkStatus {
		switch ffiConversationDebugInfo.isCommitLogForked {
		case true:
			return .forked
		case false:
			return .notForked
		default:
			return .unknown
		}
	}
}
