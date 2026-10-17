import ExampleCore
@testable import ExampleSDK
import Testing
import XmtpSdk

struct AttachmentErrorMappingTests {
	private struct Case { let cause: AttachmentFailureCause; let status: UInt16?; let message: String; let retryable: Bool
	}

	@Test(arguments: [false, true])
	func pendingAttachmentFailuresUseSDKCauseSemantics(retryable: Bool) {
		let cases: [Case] = [
			.init(
				cause: .httpStatus,
				status: retryable ? 503 : 403,
				message: "The file operation failed. Try again.",
				retryable: retryable,
			),
			.init(
				cause: .notFound,
				status: 404,
				message: "This file is not available for download. Try again later.",
				retryable: true,
			),
			.init(
				cause: .credential,
				status: nil,
				message: "File access was refused. Check your server access.",
				retryable: retryable,
			),
		]
		for item in cases {
			let failure = AttachmentFailure(
				cause: item.cause,
				credentialKind: nil,
				retryable: retryable,
				missingScope: false,
				httpStatus: item.status,
			)
			let mapped = attachmentFailure(failure)
			#expect(mapped.retryable == item.retryable)
			#expect(mapped.message == item.message)
		}
	}

	@Test func interruptedNetworkRecordAllowsAnotherUpload() {
		let failure = AttachmentFailure(
			cause: .network,
			credentialKind: nil,
			retryable: false,
			missingScope: false,
			httpStatus: nil,
		)
		#expect(attachmentFailure(failure).retryable)
	}

	@Test(arguments: [false, true])
	func thrownRetryDetailsRemainAuthoritative(retryable: Bool) {
		let failure = AttachmentFailure(
			cause: .network,
			credentialKind: nil,
			retryable: !retryable,
			missingScope: false,
			httpStatus: nil,
		)
		#expect(attachmentFailure(failure, retryable: retryable).retryable == retryable)
	}

	@Test func permanentPendingFailuresStayNonretryable() {
		for cause in [
			AttachmentFailureCause.digestMismatch,
			.malformed,
			.blockedAddress,
			.stagedUnusable,
			.sourceUnreadable,
			.tooLarge,
		] {
			let failure = AttachmentFailure(
				cause: cause,
				credentialKind: nil,
				retryable: true,
				missingScope: false,
				httpStatus: nil,
			)
			#expect(!attachmentFailure(failure).retryable)
		}
	}
}
