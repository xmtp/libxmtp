import ExampleCore
@testable import ExampleSDK
import Testing
import XmtpSdk

struct AttachmentErrorMappingTests {
	private struct Case { let cause: AttachmentFailureCause; let status: UInt16?; let message: String }
	@Test(arguments: [false, true])
	func attachmentFailuresPreserveSDKRetryDecision(retryable: Bool) {
		let cases: [Case] = [
			.init(cause: .httpStatus, status: retryable ? 503 : 403, message: "The file operation failed. Try again."),
			.init(cause: .notFound, status: 404, message: "This file is not available for download. Try again later."),
			.init(cause: .credential, status: nil, message: "File access was refused. Check your server access."),
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
			#expect(mapped.retryable == retryable)
			#expect(mapped.message == item.message)
		}
	}
}
