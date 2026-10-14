import CoreTransferable
import ExampleCore
import ImageIO
import SwiftUI
import UniformTypeIdentifiers

struct ExportDocument: Transferable {
	let url: URL

	static var transferRepresentation: some TransferRepresentation {
		FileRepresentation(exportedContentType: .data) { document in
			SentTransferredFile(document.url)
		}
	}
}

enum QuickLookExport {
	static func remove(_ url: URL) throws {
		let folder = url.deletingLastPathComponent()
		guard UUID(uuidString: folder.lastPathComponent) != nil else { return }
		if FileManager.default.fileExists(atPath: folder.path) {
			try FileManager.default.removeItem(at: folder)
		}
	}

	static func copy(_ file: VerifiedFile, directory: URL) throws -> URL {
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		let safeName = URL(fileURLWithPath: file.filename).lastPathComponent
		let folder = directory.appendingPathComponent(UUID().uuidString, isDirectory: true)
		try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
		let target = folder.appendingPathComponent(safeName); try FileManager.default.copyItem(
			at: file.url,
			to: target,
		); return target
	}
}

actor PreviewDecoder {
	static let shared = PreviewDecoder()
	private var active = 0
	private var waiting: [CheckedContinuation<Void, Never>] = []
	func decode(_ url: URL) async -> CGImage? {
		if active >= 3 {
			await withCheckedContinuation { waiting.append($0) }
		}
		active += 1
		defer {
			active -= 1; if !waiting.isEmpty {
				waiting.removeFirst().resume()
			}
		}
		guard !Task.isCancelled, let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
		return CGImageSourceCreateThumbnailAtIndex(
			source,
			0,
			[
				kCGImageSourceCreateThumbnailFromImageAlways: true,
				kCGImageSourceThumbnailMaxPixelSize: 1024,
				kCGImageSourceCreateThumbnailWithTransform: true,
			] as CFDictionary,
		)
	}
}

@MainActor enum PreviewImages { static let shared = PreviewCache<CGImage>() }
