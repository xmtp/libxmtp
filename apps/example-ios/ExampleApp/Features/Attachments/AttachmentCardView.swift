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
	var own = false
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

	@Environment(\.motionPolicy) private var motion
	var sender: String?
	var sentAt: Date?
	var messageActions = AnyView(EmptyView())
	@State private var showsMetadata = false
	@State private var showsDetails = false
	@State private var dimensions: ImageDimensions?
	@State private var localByteCount: UInt64?
	@State private var detailsFailure: String?
	/// Visible cards retain their thumbnail even when the shared reuse cache evicts it.
	@State private var renderedPreview: CGImage?

	private var preview: CGImage? {
		allowed && card.phase == .verified ? renderedPreview : nil
	}

	private var isImage: Bool {
		preview != nil || contentType?.conforms(to: .image) == true
	}

	private var contentType: UTType? {
		if let mimeType = store.verified.get(id)?.mimeType, let type = UTType(mimeType: mimeType) {
			return type
		}
		return UTType(filenameExtension: URL(fileURLWithPath: card.name).pathExtension)
	}

	private var typeLabel: String {
		let suffix = URL(fileURLWithPath: card.name).pathExtension
		return suffix.isEmpty ? localized("File") : suffix.uppercased()
	}

	var body: some View {
		Button {
			withAnimation(motion.animation) { showsMetadata.toggle() }
		} label: {
			AttachmentSurface(preview: preview, isImage: isImage, name: card.name, type: typeLabel,
			                  bytes: card.length, phase: card.phase, own: own, showsMetadata: showsMetadata)
			{
				senderAndTime
			}
		}
		.buttonStyle(.plain)
		.contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: Theme.attachmentCornerRadius))
		.contextMenu {
			messageActions
			Divider()
			fileActions
			Button(localized("Details"), systemImage: "info.circle") { showsDetails = true }
		}
		.accessibilityElement(children: .ignore)
		.accessibilityLabel(isImage ? localized("Image attachment") : card.name)
		.accessibilityValue(accessibilitySummary)
		.accessibilityHint(localized("Tap for sender and time. Touch and hold for actions."))
		.accessibilityActions {
			fileActions
			Button(localized("Details")) { showsDetails = true }
		}
		.task(id: "\(allowed)-\(card.phase.rawValue)") {
			store.send(.restore(id, descriptor, own: own))
		}
		.task(id: "\(allowed)-\(card.phase.rawValue)-\(graph.consentRevision)") { await loadPreview() }
		.sheet(isPresented: $showsDetails) { details }
		.alert(localized("Attachment action failed"), isPresented: Binding(
			get: { failure != nil }, set: {
				if !$0 {
					failure = nil
				}
			},
		)) {
			Button(localized("Close"), role: .cancel) { failure = nil }
		} message: { Text(localized(failure ?? "The file operation failed. Try again.")) }
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
		.onDisappear { renderedPreview = nil; copyID = nil; copyTask?.cancel() }
	}

	private func loadPreview() async {
		renderedPreview = nil
		guard allowed, card.phase == .verified, let file = store.verified.get(id),
		      file.mimeType?.hasPrefix("image/") == true, let key = graph.active?.key else { return }
		let origin = DestinationOrigin(session: key, token: graph.routes.token)
		var decoded: CGImage?
		await PreviewImages.shared.load(id: id) {
			let image = await PreviewDecoder.shared.decode(file.url)
			decoded = image
			return image
		}
		guard !Task.isCancelled, allowed, card.phase == .verified,
		      graph.routes.admit(origin, fence: graph.fence) else { return }
		renderedPreview = PreviewImages.shared.values[id] ?? decoded
	}

	@ViewBuilder private var fileActions: some View {
		if card.phase == .verified {
			Button(localized("Open"), systemImage: "arrow.up.forward.app") { open(save: false) }
				.disabled(!allowed || copyID != nil)
			Button(localized("Save"), systemImage: Symbol.save) { open(save: true) }
				.disabled(!allowed || copyID != nil)
		} else if allowed, card.phase != .downloading, card.phase != .failed || card.retryable {
			Button(localized(card.phase == .failed ? "Retry" : "Download"), systemImage: Symbol.download) {
				store.send(.download(id, descriptor))
			}
		}
	}

	private func localized(_ key: String) -> String {
		String(localized: String.LocalizationValue(key), table: "Attachments")
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

private extension AttachmentCardView {
	var senderAndTime: some View {
		ViewThatFits(in: .horizontal) {
			HStack(spacing: 6) { senderLabel; timestamp }
			VStack(alignment: .leading, spacing: 4) { senderLabel; timestamp }
		}
		.font(.caption)
	}

	@ViewBuilder var senderLabel: some View {
		if own {
			Text("You", tableName: "Attachments")
		} else if let sender {
			IdentityText(inboxID: sender, handle: graph.identityDirectory?.identities[sender]?.handle, short: true)
		}
	}

	@ViewBuilder var timestamp: some View {
		if let sentAt {
			Text(sentAt, format: .dateTime.hour().minute())
		}
	}

	var details: some View {
		NavigationStack {
			Form {
				Section {
					LabeledContent(localized("Filename"), value: card.name)
					LabeledContent(localized("Type"), value: contentType?.localizedDescription ?? typeLabel)
					if let localByteCount {
						LabeledContent(localized("Size"), value: ByteCountFormatter.string(
							fromByteCount: Int64(clamping: localByteCount), countStyle: .file,
						))
					} else if let bytes = card.length {
						LabeledContent(localized("Download size"), value: AttachmentSurface<EmptyView>.formattedBytes(bytes))
					}
					if let dimensions {
						LabeledContent(localized("Dimensions"), value: "\(dimensions.width) × \(dimensions.height)")
					}
					LabeledContent(localized("Status"), value: localized(status))
				}
				if sender != nil || sentAt != nil {
					Section { senderAndTime }
				}
				if let error = card.error ?? failure ?? detailsFailure {
					Section { Text(localized(error)).foregroundStyle(.red) }
				}
			}
			.navigationTitle(localized("Attachment details"))
			.navigationBarTitleDisplayMode(.inline)
			.toolbar {
				ToolbarItem(placement: .cancellationAction) {
					Button(localized("Close"), systemImage: Symbol.close) { showsDetails = false }
						.labelStyle(.iconOnly).buttonStyle(.glass)
				}
			}
		}
		.presentationDetents([.medium, .large])
		.task(id: "\(allowed)-\(card.phase.rawValue)") {
			dimensions = nil; localByteCount = nil; detailsFailure = nil
			guard allowed, card.phase == .verified, let file = store.verified.get(id),
			      let key = graph.active?.key else { return }
			let origin = DestinationOrigin(session: key, token: graph.routes.token)
			do {
				let bytes = try await PreviewDecoder.shared.byteCount(file.url)
				let value = isImage ? await PreviewDecoder.shared.dimensions(file.url) : nil
				guard !Task.isCancelled, allowed, graph.routes.admit(origin, fence: graph.fence) else { return }
				localByteCount = bytes; dimensions = value
			} catch is CancellationError {} catch {
				guard !Task.isCancelled, allowed, graph.routes.admit(origin, fence: graph.fence) else { return }
				detailsFailure = "Cannot load file details. Try again."
			}
		}
	}

	var accessibilitySummary: String {
		var parts = [localized(status)]
		if !isImage {
			parts.append(typeLabel)
			if let bytes = card.length {
				parts.append(AttachmentSurface<EmptyView>.formattedBytes(bytes))
			}
		}
		if showsMetadata {
			if own {
				parts.append(localized("You"))
			} else if let sender {
				parts.append(graph.identityDirectory?.identities[sender]?.handle ?? sender)
			}
			if let sentAt {
				parts.append(sentAt.formatted(date: .omitted, time: .shortened))
			}
		}
		return parts.joined(separator: ", ")
	}
}
