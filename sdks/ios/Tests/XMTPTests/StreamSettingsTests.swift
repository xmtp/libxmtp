import XCTest
@testable import XMTPiOS

final class StreamSettingsTests: XCTestCase {
	func testUnsetFieldsUseNativeDefaults() {
		let settings = StreamSettings().toFfi()
		XCTAssertNil(settings.maxAdmissionRows)
		XCTAssertNil(settings.maxAdmissionBytes)
		XCTAssertNil(settings.maxFetchedRows)
		XCTAssertNil(settings.maxFetchedBytes)
		XCTAssertNil(settings.groupPendingRows)
		XCTAssertNil(settings.groupPendingBytes)
		XCTAssertNil(settings.welcomePendingRows)
		XCTAssertNil(settings.welcomePendingBytes)
		XCTAssertNil(settings.identityPendingRows)
		XCTAssertNil(settings.identityPendingBytes)
		XCTAssertNil(settings.maxPendingRowsPerTopic)
		XCTAssertNil(settings.maxPendingBytesPerTopic)
		XCTAssertNil(settings.maxDependencyRequests)
		XCTAssertNil(settings.maxLocalReadRows)
		XCTAssertNil(settings.maxLocalReadBytes)
		XCTAssertNil(settings.receiverFallbackIntervalMs)
		XCTAssertNil(settings.activeDatabasePollIntervalMs)
		XCTAssertNil(settings.defaultConsumerLeaseDurationMs)
		XCTAssertNil(settings.identityReferenceWaitMs)
		XCTAssertNil(settings.barrierTimeoutMs)
	}

	func testSuppliedFieldsKeepTheirValuesAndNativeIntegerWidths() {
		let settings = StreamSettings(
			maxAdmissionRows: UInt32.max,
			maxAdmissionBytes: UInt64.max,
			maxFetchedRows: 3,
			maxFetchedBytes: 4,
			groupPendingRows: 5,
			groupPendingBytes: 6,
			welcomePendingRows: 7,
			welcomePendingBytes: 8,
			identityPendingRows: 9,
			identityPendingBytes: 10,
			maxPendingRowsPerTopic: 11,
			maxPendingBytesPerTopic: 12,
			maxDependencyRequests: 13,
			maxLocalReadRows: 14,
			maxLocalReadBytes: 15,
			receiverFallbackIntervalMs: 16,
			activeDatabasePollIntervalMs: 17,
			defaultConsumerLeaseDurationMs: 18,
			identityReferenceWaitMs: 19,
			barrierTimeoutMs: 20
		).toFfi()
		XCTAssertEqual(settings.maxAdmissionRows, UInt32.max)
		XCTAssertEqual(settings.maxAdmissionBytes, UInt64.max)
		XCTAssertEqual(settings.maxFetchedRows, 3)
		XCTAssertEqual(settings.maxFetchedBytes, 4)
		XCTAssertEqual(settings.groupPendingRows, 5)
		XCTAssertEqual(settings.groupPendingBytes, 6)
		XCTAssertEqual(settings.welcomePendingRows, 7)
		XCTAssertEqual(settings.welcomePendingBytes, 8)
		XCTAssertEqual(settings.identityPendingRows, 9)
		XCTAssertEqual(settings.identityPendingBytes, 10)
		XCTAssertEqual(settings.maxPendingRowsPerTopic, 11)
		XCTAssertEqual(settings.maxPendingBytesPerTopic, 12)
		XCTAssertEqual(settings.maxDependencyRequests, 13)
		XCTAssertEqual(settings.maxLocalReadRows, 14)
		XCTAssertEqual(settings.maxLocalReadBytes, 15)
		XCTAssertEqual(settings.receiverFallbackIntervalMs, 16)
		XCTAssertEqual(settings.activeDatabasePollIntervalMs, 17)
		XCTAssertEqual(settings.defaultConsumerLeaseDurationMs, 18)
		XCTAssertEqual(settings.identityReferenceWaitMs, 19)
		XCTAssertEqual(settings.barrierTimeoutMs, 20)
	}
}
