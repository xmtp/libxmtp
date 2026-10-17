@testable import ExampleSDK
import Testing
import XmtpSdk

@Test func handleErrorMapping() {
	let detail = ErrorDetails(code: "test", category: .identity, retryable: false, message: "test")
	#expect(mapHandleError(XmtpError.HandleTaken(detail)) == .taken)
	#expect(mapHandleError(XmtpError.HandleNotAllowed(detail)) == .notAllowed)
	#expect(mapHandleError(XmtpError.HandleTooSoon(detail)) == .tooSoon)
	#expect(mapHandleError(XmtpError.HandleLimitReached(detail)) == .limitReached)
	#expect(mapHandleError(XmtpError.IdentityNotFound(detail)) == .failed)
	#expect(lookupIsNotFound(XmtpError.IdentityNotFound(detail)))
	#expect(!lookupIsNotFound(XmtpError.HandleTaken(detail)))
}
