import ExampleCore
import QuickLook
import SwiftUI
import UniformTypeIdentifiers

struct AttachmentCardView: View {
	let id: String
	let descriptor: FileDescriptor
	let store: AttachmentStore
	let graph: AppGraph
	let allowed: Bool
	@State private var quickLook: URL?
	@State private var export: ExportDocument?
	@State private var exporting =
		false
	@State private var failure: String?
	@State private var copyTask: Task<Void, Never>?
	@State private var copyID: UUID?
	private var card: FileCard {
		store.state.cards[id] ?? FileCard(
			id: id,
			name: descriptor.filename ?? "File",
			length: descriptor.length,
		)
	}

	var body: some View {
		VStack(alignment: .leading, spacing: 8) {
			if let preview = PreviewImages.shared.values[id] {
				Image(decorative: preview, scale: 1).resizable().scaledToFit().frame(maxHeight: 220)
					.clipShape(RoundedRectangle(cornerRadius: 12))
			}
			Label(card.name, systemImage: "doc.fill").font(.subheadline.weight(.medium))
			HStack {
				if let bytes = card
					.length
				{
					Text(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file))
				}
				Text(String(localized: String.LocalizationValue(status), table: "Attachments"))
			}
			.font(.caption).foregroundStyle(.secondary)
			if card
				.phase ==
				.verified
			{
				ViewThatFits(in: .horizontal) {
					HStack { fileActions }
					VStack(alignment: .leading) { fileActions }
				}.buttonStyle(.glass)
			} else if card.phase == .downloading {
				ProgressView().accessibilityLabel("Downloading")
			} else if allowed,
			          card.phase != .failed || card
			          .retryable
			{
				Button(card.phase == .failed ? "Retry" : "Download") { store.send(.download(
					id,
					descriptor,
				)) }.buttonStyle(.glass)
			}
			if let error = card.error ?? failure {
				Text(String(localized: String.LocalizationValue(error), table: "Attachments")).font(.caption).foregroundStyle(.red)
			}
		}.padding(12).frame(maxWidth: 260, alignment: .leading)
			.task(id: card.phase) {
				if let file = store.verified.get(id),
				   file.mimeType?
				   .hasPrefix("image/") == true
				{
					await PreviewImages.shared.load(id: id) { await PreviewDecoder.shared.decode(file.url) }
				}
			}
			.quickLookPreview($quickLook)
			.onChange(of: quickLook) { previous, current in
				if let previous, previous != current {
					removeExport(previous)
				}
			}
			.fileExporter(isPresented: $exporting, item: export, contentTypes: [.data], defaultFilename: card.name) { result in
				if case .failure = result {
					failure = "Cannot save this file. Try again."
				}
				clearExport()
			} onCancellation: { clearExport() }
			.onDisappear { copyID = nil; copyTask?.cancel() }
	}

	@ViewBuilder private var fileActions: some View {
		Button(String(localized: "Open", table: "Attachments")) { open(save: false) }.disabled(!allowed || copyID != nil)
		Button(String(localized: "Save", table: "Attachments")) { open(save: true) }.disabled(!allowed || copyID != nil)
	}

	private var status: String {
		if !allowed {
			return "Closed until you accept"
		}
		switch card
			.phase
		{
		case .available: return "Download to verify"
		case .downloading: return "Downloading"
		case .verified: return "Verified"
		case .failed: return "Failed"
		default: return card
			.phase.rawValue.capitalized
		}
	}

	private func removeExport(_ url: URL) {
		do { try QuickLookExport.remove(url) } catch { failure = "Cannot remove this temporary file. Try again." }
	}

	private func clearExport() {
		if let export {
			removeExport(export.url)
		}
		export = nil
	}

	private func open(save: Bool) {
		guard allowed, quickLook == nil, !exporting, copyID == nil,
		      let file = store.verified.get(id), let key = graph.active?.key else { return }
		clearExport()
		do {
			let directory = try graph.files.paths(key.profileID).exports
			let origin = DestinationOrigin(session: key, token: graph.routes.token)
			let operation = UUID()
			copyID = operation
			copyTask = graph.session.scope.add {
				defer {
					if copyID == operation {
						copyID = nil; copyTask = nil
					}
				}
				do {
					let url = try await QuickLookExport.copy(file, directory: directory)
					guard !Task.isCancelled, copyID == operation, graph.routes.admit(origin, fence: graph.fence) else {
						try QuickLookExport.remove(url)
						return
					}
					if save {
						export = ExportDocument(url: url); exporting = true
					} else {
						quickLook = url
					}
				} catch is CancellationError { } catch {
					if !Task.isCancelled, copyID == operation, graph.routes.admit(origin, fence: graph.fence) {
						failure = "Cannot open this file. Try again."
					}
				}
			}
		} catch { failure = "Cannot open this file. Try again." }
	}
}
