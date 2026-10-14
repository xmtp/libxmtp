import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
import XmtpSdk

extension SDKMappingTests {
	@Test(arguments: ["fixture-access-token", "  fixture-access-token \n"])
	func sessionCredentialUsesCompleteBearerHeader(token: String) throws {
		let credential = try bearerCredential(token)
		#expect(credential.name == nil)
		#expect(credential.value == "Bearer fixture-access-token")
		#expect(credential.expiresAtSeconds > Int64(Date().timeIntervalSince1970))
	}

	@Test(arguments: ["", " \n\t"])
	func emptyBearerTokenIsRejected(token: String) {
		#expect(throws: SessionError.message("Enter an access token.")) { try bearerCredential(token) }
	}

	@Test func credentialRejectionHasUsefulConnectCopy() {
		let failure = XmtpError.CredentialRejected(ErrorDetails(
			code: "CredentialRejected",
			category: .network,
			retryable: true,
			message: "private wire diagnostic",
		))
		#expect((connectionError(failure) as? SessionError) ==
			.message("Access token was not accepted. Check it and try again."))
	}
}
