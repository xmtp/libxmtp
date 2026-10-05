import Foundation
import XCTest
import XmtpSdk

private let callbackSecret = "swift-callback-secret-sentinel"

private struct SecretFailure: Error, CustomStringConvertible, LocalizedError {
	var description: String {
		"refresh failed: \(callbackSecret)"
	}

	var errorDescription: String? {
		description
	}
}

private final class CountingCredentialSource: CredentialSource, @unchecked Sendable {
	let calls = Shared(0)
	let fail: Bool

	init(fail: Bool) {
		self.fail = fail
	}

	func credential() async throws -> Credential {
		calls.update { $0 += 1 }
		if fail {
			throw SecretFailure()
		}
		return Credential(
			name: nil, value: "Bearer swift-credential-callback",
			expiresAtSeconds: Int64(Date().timeIntervalSince1970) + 3600,
		)
	}
}

/// A signer whose identity is real and whose signature always fails.
private final class FailingSigner: Signer, @unchecked Sendable {
	let inner: Signer

	init(_ inner: Signer) {
		self.inner = inner
	}

	func identity() async throws -> PublicIdentity {
		try await inner.identity()
	}

	func kind() async throws -> SignerKind {
		try await inner.kind()
	}

	func sign(request _: SigningRequest) async throws -> Signature {
		throw SecretFailure()
	}
}

/// Swift callbacks that cross the FFI: a credential source and a signer. A
/// callback failure is typed, and its text never reaches the app error.
final class AuthenticationCallbackTests: XCTestCase {
	func testCredentialSourceSuppliesTheBackendCredential() async throws {
		let source = CountingCredentialSource(fail: false)
		var options = liveOptions()
		options.backend = .options(options: BackendOptions(url: liveBackendURL, credentials: source))
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
		XCTAssertGreaterThan(source.calls.value, 0, "Client creation did not ask the credential source")
		let reachable = try await client.canMessage(identities: [client.identity()])
		XCTAssertEqual(reachable.values.first, true)
		try await client.end()
	}

	func testCredentialSourceFailureIsTypedAndHidesItsText() async throws {
		let source = CountingCredentialSource(fail: true)
		var options = liveOptions()
		options.backend = .options(options: BackendOptions(url: liveBackendURL, credentials: source))
		do {
			let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
			try await client.end()
			XCTFail("A failed credential source allowed client creation")
		} catch let XmtpError.CredentialCallbackFailed(details) {
			XCTAssertEqual(details.category, .callback)
			assertHidden(XmtpError.CredentialCallbackFailed(details), details)
		}
		XCTAssertGreaterThan(source.calls.value, 0)
	}

	func testThrowingSignerFailureIsTypedAndHidesItsText() async throws {
		let signer = await FailingSigner(generateLocalSigner())
		do {
			let client = try await SDKClient.create(signer: signer, options: liveOptions())
			try await client.end()
			XCTFail("A failed signature registered the client")
		} catch let XmtpError.Signer(details) {
			XCTAssertEqual(details.category, .callback)
			assertHidden(XmtpError.Signer(details), details)
		}
	}

	private func assertHidden(_ error: Error, _ details: ErrorDetails, line: UInt = #line) {
		let forms = [
			details.message,
			String(describing: error),
			String(reflecting: error),
			error.localizedDescription,
			"\(details)",
		]
		for form in forms {
			XCTAssertFalse(form.contains(callbackSecret), "The callback text reached the app: \(form)", line: line)
		}
	}
}
