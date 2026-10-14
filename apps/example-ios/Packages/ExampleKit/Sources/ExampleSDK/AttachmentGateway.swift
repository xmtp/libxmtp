import ExampleCore
import Foundation
import UniformTypeIdentifiers
import XmtpSdk

func attachmentFailure(_ failure: AttachmentFailure, retryable: Bool? = nil) -> ExampleCore.AttachmentError {
	let message = switch failure.cause {
	case .notOffered: "This server does not support file uploads."
	case .tooLarge: "This file exceeds the server size limit."
	case .stagedUnusable, .deleted: "This file is unavailable. Select the file again."
	case .notFound: "This file is not available for download. Try again later."
	case .digestMismatch, .decryptionFailed, .notAnAttachment, .malformed: "This file could not be verified."
	case .blockedAddress, .insecureUrl: "This file location is not allowed."
	case .credential: "File access was refused. Check your server access."
	default: "The file operation failed. Try again."
	}
	// The record's retryable flag describes credentials. Other pending failures
	// use the SDK's stable cause table; thrown errors supply their own detail.
	let canRetry = retryable ?? attachmentCanRetry(failure)
	return .failure(message: message, retryable: canRetry)
}

private func attachmentCanRetry(_ failure: AttachmentFailure) -> Bool {
	switch failure.cause {
	case .localStorage, .backendUnavailable, .network, .targetRejected, .notFound, .deleted: true
	case .credential: failure.retryable
	case .httpStatus: failure.httpStatus.map { $0 == 408 || $0 == 429 || (500 ... 599).contains($0) } ?? false
	default: false
	}
}

public final class SDKAttachmentGateway: AttachmentGateway, Sendable {
	private let client: SDKClient
	public init(client: SDKClient) {
		self.client = client
	}

	public func capability() async throws -> UInt64? {
		guard client.attachments().offered()
		else { return nil }
		return client.serverConfiguration().attachments?.maxUploadBytes
	}

	public func create(file: URL,
	                   filename: String) async throws
		-> FileDescriptor
	{
		do
		{ return try await coreFile(client.attachments().create(source: .path(
			path: file.path,
			filename: filename,
			mimeType: attachmentMIMEType(filename: filename),
		)).remoteAttachment()) } catch { throw mapped(error) }
	}

	public func pending() async throws
		-> [FileDescriptor]
	{
		do { return try await client.attachments().listPending().map { coreFile($0.remoteAttachment()) } } catch {
			throw mapped(error)
		}
	}

	public func upload(_ file: FileDescriptor) async throws -> FilePhase {
		do {
			let pending = try await client.attachments().pending(remote: sdkFile(file))
			try await pending
				.upload()
			switch try await pending
				.status()
			{
			case .waiting: return .waiting
			case .uploading: return .uploading
			case .complete: return .complete
			case let .failed(
				error,
			): throw attachmentFailure(error)
			}
		} catch { throw mapped(error) }
	}

	public func download(_ file: FileDescriptor) async throws
		-> VerifiedFile
	{
		do { let result = try await client.attachments().download(remote: sdkFile(file))
			return VerifiedFile(
				url: URL(fileURLWithPath: result.path),
				mimeType: result.mimeType,
				filename: result.filename ?? file.filename ?? "File",
			)
		} catch { throw mapped(error) }
	}

	public func discard(_ file: FileDescriptor) async throws {
		do { try await client.attachments().deleteLocal(remote: sdkFile(file)) } catch { throw mapped(error) }
	}

	private func mapped(_ error: any Error) -> any Error {
		if let error = error as? ExampleCore
			.AttachmentError
		{
			return error
		}
		if case let XmtpError.Attachment(
			details,
			failure,
		) = error {
			return attachmentFailure(failure, retryable: details.retryable)
		}
		return ExampleCore.AttachmentError.failure(
			message: "The file operation failed. Try again.",
			retryable: true,
		)
	}
}

func attachmentMIMEType(filename: String)
	-> String
{
	UTType(filenameExtension: URL(fileURLWithPath: filename).pathExtension)?
		.preferredMIMEType ?? "application/octet-stream"
}
