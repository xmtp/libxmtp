import CoreTransferable
import ExampleCore
import Foundation
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ImportedPhoto: Transferable {
	let url: URL
	let filename: String
	static var transferRepresentation: some TransferRepresentation {
		FileRepresentation(importedContentType: .data) { incoming in
			let staged = try await PhotoImportContext.stage(incoming.file)
			return ImportedPhoto(url: staged, filename: incoming.file.lastPathComponent)
		}
	}
}

enum PhotoImportContext {
	private struct Context: Sendable {
		let directory: URL
		let limit: UInt64
		let files: AttachmentImportFiles
	}

	private static let gate = AttachmentImportGate<Context>()
	static func load(_ item: PhotosPickerItem, directory: URL, limit: UInt64) async throws -> ImportedPhoto? {
		let files = AttachmentImportFiles(owner: AttachmentStagingFiles(temporaryDirectory: directory))
		return try await gate.withContext(Context(directory: directory, limit: limit, files: files)) {
			do {
				let result = try await item.loadTransferable(type: ImportedPhoto.self)
				if result == nil || Task.isCancelled {
					try await files.close()
				}
				try Task.checkCancellation()
				return result
			} catch {
				try await files.close()
				throw error
			}
		}
	}

	static func stage(_ source: URL) async throws -> URL {
		guard let context = await gate.current else { throw CocoaError(.fileReadUnknown) }
		let file = try FileStager.stage(source, directory: context.directory, limit: context.limit)
		try await context.files.record(file)
		return file
	}
}

@MainActor final class PhotoImportController {
	private var task: Task<Void, Never>?
	struct Callbacks {
		let admitted: @MainActor () -> Bool
		let receive: @MainActor (ImportedPhoto) -> Void
		let failure: @MainActor (any Error) -> Void
	}

	func start(_ item: PhotosPickerItem, directory: URL, limit: UInt64, callbacks: Callbacks) {
		task?.cancel()
		let previous = task
		task = Task {
			await previous?.value
			guard !Task.isCancelled, callbacks.admitted() else { return }
			do {
				guard let file = try await PhotoImportContext.load(item, directory: directory, limit: limit) else { return }
				guard !Task.isCancelled, callbacks.admitted() else {
					try AttachmentStagingFiles(temporaryDirectory: directory).removeOwned(file.url)
					return
				}
				callbacks.receive(file)
			} catch is CancellationError { } catch {
				if callbacks.admitted() {
					callbacks.failure(error)
				}
			}
		}
	}

	func close() async {
		task?.cancel(); await task?.value
	}
}
