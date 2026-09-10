/// Optional stream limits. Nil fields use native defaults. All timer values are milliseconds.
/// Native client creation validates the supplied values.
public struct StreamSettings {
	public var maxAdmissionRows: UInt32?
	public var maxAdmissionBytes: UInt64?
	public var maxFetchedRows: UInt32?
	public var maxFetchedBytes: UInt64?
	public var groupPendingRows: UInt64?
	public var groupPendingBytes: UInt64?
	public var welcomePendingRows: UInt64?
	public var welcomePendingBytes: UInt64?
	public var identityPendingRows: UInt64?
	public var identityPendingBytes: UInt64?
	public var maxPendingRowsPerTopic: UInt64?
	public var maxPendingBytesPerTopic: UInt64?
	public var maxDependencyRequests: UInt32?
	public var maxLocalReadRows: UInt32?
	public var maxLocalReadBytes: UInt64?
	public var receiverFallbackIntervalMs: UInt64?
	public var activeDatabasePollIntervalMs: UInt64?
	public var defaultConsumerLeaseDurationMs: UInt64?
	public var identityReferenceWaitMs: UInt64?
	public var barrierTimeoutMs: UInt64?

	public init(
		maxAdmissionRows: UInt32? = nil,
		maxAdmissionBytes: UInt64? = nil,
		maxFetchedRows: UInt32? = nil,
		maxFetchedBytes: UInt64? = nil,
		groupPendingRows: UInt64? = nil,
		groupPendingBytes: UInt64? = nil,
		welcomePendingRows: UInt64? = nil,
		welcomePendingBytes: UInt64? = nil,
		identityPendingRows: UInt64? = nil,
		identityPendingBytes: UInt64? = nil,
		maxPendingRowsPerTopic: UInt64? = nil,
		maxPendingBytesPerTopic: UInt64? = nil,
		maxDependencyRequests: UInt32? = nil,
		maxLocalReadRows: UInt32? = nil,
		maxLocalReadBytes: UInt64? = nil,
		receiverFallbackIntervalMs: UInt64? = nil,
		activeDatabasePollIntervalMs: UInt64? = nil,
		defaultConsumerLeaseDurationMs: UInt64? = nil,
		identityReferenceWaitMs: UInt64? = nil,
		barrierTimeoutMs: UInt64? = nil
	) {
		self.maxAdmissionRows = maxAdmissionRows
		self.maxAdmissionBytes = maxAdmissionBytes
		self.maxFetchedRows = maxFetchedRows
		self.maxFetchedBytes = maxFetchedBytes
		self.groupPendingRows = groupPendingRows
		self.groupPendingBytes = groupPendingBytes
		self.welcomePendingRows = welcomePendingRows
		self.welcomePendingBytes = welcomePendingBytes
		self.identityPendingRows = identityPendingRows
		self.identityPendingBytes = identityPendingBytes
		self.maxPendingRowsPerTopic = maxPendingRowsPerTopic
		self.maxPendingBytesPerTopic = maxPendingBytesPerTopic
		self.maxDependencyRequests = maxDependencyRequests
		self.maxLocalReadRows = maxLocalReadRows
		self.maxLocalReadBytes = maxLocalReadBytes
		self.receiverFallbackIntervalMs = receiverFallbackIntervalMs
		self.activeDatabasePollIntervalMs = activeDatabasePollIntervalMs
		self.defaultConsumerLeaseDurationMs = defaultConsumerLeaseDurationMs
		self.identityReferenceWaitMs = identityReferenceWaitMs
		self.barrierTimeoutMs = barrierTimeoutMs
	}

	func toFfi() -> FfiStreamSettings {
		FfiStreamSettings(
			maxAdmissionRows: maxAdmissionRows,
			maxAdmissionBytes: maxAdmissionBytes,
			maxFetchedRows: maxFetchedRows,
			maxFetchedBytes: maxFetchedBytes,
			groupPendingRows: groupPendingRows,
			groupPendingBytes: groupPendingBytes,
			welcomePendingRows: welcomePendingRows,
			welcomePendingBytes: welcomePendingBytes,
			identityPendingRows: identityPendingRows,
			identityPendingBytes: identityPendingBytes,
			maxPendingRowsPerTopic: maxPendingRowsPerTopic,
			maxPendingBytesPerTopic: maxPendingBytesPerTopic,
			maxDependencyRequests: maxDependencyRequests,
			maxLocalReadRows: maxLocalReadRows,
			maxLocalReadBytes: maxLocalReadBytes,
			receiverFallbackIntervalMs: receiverFallbackIntervalMs,
			activeDatabasePollIntervalMs: activeDatabasePollIntervalMs,
			defaultConsumerLeaseDurationMs: defaultConsumerLeaseDurationMs,
			identityReferenceWaitMs: identityReferenceWaitMs,
			barrierTimeoutMs: barrierTimeoutMs
		)
	}
}
