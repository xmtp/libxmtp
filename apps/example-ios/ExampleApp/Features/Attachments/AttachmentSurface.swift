import ExampleCore
import SwiftUI

/// Attachment presentation without download or export controls in the transcript.
struct AttachmentSurface<Metadata: View>: View {
	let preview: CGImage?
	let isImage: Bool
	let name: String
	let type: String
	let bytes: UInt32?
	let phase: FilePhase
	let own: Bool
	let showsMetadata: Bool
	@ViewBuilder let metadata: () -> Metadata

	private var aspectRatio: CGFloat {
		guard let preview else { return 4 / 3 }
		let ratio = CGFloat(preview.width) / CGFloat(max(1, preview.height))
		return min(Theme.attachmentAspectRatio.upperBound, max(Theme.attachmentAspectRatio.lowerBound, ratio))
	}

	var body: some View {
		Group {
			if isImage {
				image
			} else {
				VStack(alignment: .leading, spacing: 6) {
					file
					if showsMetadata {
						metadata().foregroundStyle(.secondary).padding(.horizontal, 12).transition(.opacity)
					}
				}
			}
		}
		.frame(maxWidth: Theme.attachmentMaximumWidth, alignment: .leading)
	}

	private var image: some View {
		Color(uiColor: .secondarySystemBackground)
			.aspectRatio(aspectRatio, contentMode: .fit)
			.overlay {
				if let preview {
					Image(decorative: preview, scale: 1).resizable().scaledToFill()
				} else {
					VStack(spacing: 12) {
						Image(systemName: Symbol.photo).font(.largeTitle)
						statusIcon
					}.foregroundStyle(.secondary)
				}
			}
			.overlay(alignment: .bottomLeading) {
				if showsMetadata {
					metadata().foregroundStyle(.white).frame(maxWidth: .infinity, alignment: .leading)
						.padding(.horizontal, 14).padding(.bottom, 12).padding(.top, 30)
						.background(LinearGradient(colors: [.clear, .black.opacity(0.75)], startPoint: .top, endPoint: .bottom))
						.transition(.opacity)
				}
			}
			.clipShape(RoundedRectangle(cornerRadius: Theme.attachmentCornerRadius))
	}

	private var file: some View {
		HStack(alignment: .center, spacing: 12) {
			Image(systemName: Symbol.file).font(.title2)
				.frame(width: 44, height: 48)
				.background(own ? Color.white.opacity(0.16) : Color.primary.opacity(0.06),
				            in: RoundedRectangle(cornerRadius: 10))
			VStack(alignment: .leading, spacing: 4) {
				Text(name).font(.subheadline.weight(.medium)).lineLimit(2)
				Text(fileSummary).font(.caption).foregroundStyle(own ? Color.white.opacity(0.8) : .secondary)
			}
			.frame(maxWidth: .infinity, alignment: .leading)
			statusIcon
		}
		.padding(14)
		.foregroundStyle(own ? Color.white : .primary)
		.background(own ? Theme.tint : Theme.incomingBubble,
		            in: RoundedRectangle(cornerRadius: Theme.attachmentCornerRadius))
	}

	@ViewBuilder private var statusIcon: some View {
		if phase == .downloading {
			ProgressView().tint(own && !isImage ? .white : nil)
		} else {
			Image(systemName: phase == .verified ? "checkmark" : phase == .failed ? Symbol.error : Symbol.download)
				.font(.caption.weight(.semibold))
		}
	}

	private var fileSummary: String {
		guard let bytes else { return type }
		return type + " · " + Self.formattedBytes(bytes)
	}

	static func formattedBytes(_ bytes: UInt32) -> String {
		ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
	}
}
