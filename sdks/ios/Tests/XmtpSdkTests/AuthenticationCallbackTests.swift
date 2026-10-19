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

/// A pre-authentication handler that counts its runs and always fails.
private final class FailingPreAuthenticate: PreAuthenticate, @unchecked Sendable {
	let calls = Shared(0)

	func run() async throws {
		calls.update { $0 += 1 }
		throw PreAuthenticateError.Failed
	}
}

/// A signer that counts its signatures.
private final class CountingSigner: Signer, @unchecked Sendable {
	let inner: Signer
	let signatures = Shared(0)

	init(_ inner: Signer) {
		self.inner = inner
	}

	func identity() async throws -> PublicIdentity {
		try await inner.identity()
	}

	func kind() async throws -> SignerKind {
		try await inner.kind()
	}

	func sign(request: SigningRequest) async throws -> Signature {
		signatures.update { $0 += 1 }
		return try await inner.sign(request: request)
	}
}

/// Swift callbacks that cross the FFI: a credential source, a
/// pre-authentication handler and a signer. A
/// callback failure is typed, and its text never reaches the app error.
final class AuthenticationCallbackTests: XCTestCase {
	func testCredentialSourceSuppliesTheBackendCredential() async throws {
		let source = CountingCredentialSource(fail: false)
		var options = liveOptions()
		options.backend = .options(options: BackendOptions(url: liveBackendURL, credentials: source))
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner(), options: options)
			XCTAssertGreaterThan(source.calls.value, 0, "Client creation did not ask the credential source")
			let reachable = try await client.canMessage(identities: [client.identity()])
			XCTAssertEqual(reachable.values.first, true)
		}
	}

	func testCredentialSourceFailureIsTypedAndHidesItsText() async throws {
		let source = CountingCredentialSource(fail: true)
		var options = liveOptions()
		options.backend = .options(options: BackendOptions(url: liveBackendURL, credentials: source))
		do {
			try await withClients { scope in
				_ = try await scope.create(signer: generateLocalSigner(), options: options)
				XCTFail("A failed credential source allowed client creation")
			}
		} catch let XmtpError.CredentialCallbackFailed(details) {
			XCTAssertEqual(details.category, .callback)
			assertHidden(XmtpError.CredentialCallbackFailed(details), details)
		}
		XCTAssertGreaterThan(source.calls.value, 0)
	}

	func testThrowingSignerFailureIsTypedAndHidesItsText() async throws {
		let signer = await FailingSigner(generateLocalSigner())
		do {
			try await withClients { scope in
				_ = try await scope.create(signer: signer)
				XCTFail("A failed signature registered the client")
			}
		} catch let XmtpError.Signer(details) {
			XCTAssertEqual(details.category, .callback)
			assertHidden(XmtpError.Signer(details), details)
		}
	}

	func testPreAuthenticateFailureIsTypedAndStopsRegistration() async throws {
		let preAuthenticate = FailingPreAuthenticate()
		let signer = await CountingSigner(generateLocalSigner())
		var options = liveOptions()
		options.handlers = ClientHandlers(preAuthenticate: preAuthenticate)
		do {
			try await withClients { scope in
				_ = try await scope.create(signer: signer, options: options)
				XCTFail("A failed preAuthenticate handler allowed registration")
			}
		} catch let XmtpError.CallbackFailed(details) {
			XCTAssertEqual(details.code, "CallbackFailed")
			XCTAssertEqual(details.category, .callback)
			XCTAssertFalse(details.retryable)
		}
		XCTAssertEqual(preAuthenticate.calls.value, 1)
		XCTAssertEqual(signer.signatures.value, 0, "The signer ran after preAuthenticate failed")
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
