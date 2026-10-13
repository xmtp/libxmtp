import ExampleCore
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

extension ChatContentView {
	var composer: some View {
		GlassEffectContainer {
			VStack(spacing: 8) {
				ForEach(runtime.attachments.state.drafts.filter { $0.conversationID == id }) { draft in
					HStack {
						Text(draft.card.name).lineLimit(1)
						Text(draft.phase.rawValue.capitalized).font(.caption)
						if draft.phase == .unknown {
							Button(String(localized: "Review send", table: "Chat")) { graph.navigate(.attachmentRecovery) }
								.buttonStyle(.glass)
						} else if draft.phase == .uploading {
							ProgressView().accessibilityLabel(String(localized: "Uploading", table: "Attachments"))
						} else if draft.phase != .failed || draft.retryable {
							Button(String(localized: String.LocalizationValue(draft.phase == .failed ? "Retry" : "Send"), table: "Chat")) {
								runtime.attachments.send(.send(draft.id))
							}
							.buttonStyle(.glassProminent).disabled(!allowed)
						}
						Button(String(localized: "Discard", table: "Chat"), role: .destructive) {
							runtime.attachments.send(.discard(draft.id))
						}.buttonStyle(.glass)
					}
				}
				if let error = runtime.composer.state.error ?? runtime.attachments.state
					.error ?? error
				{
					Text(String(localized: String.LocalizationValue(error), table: "Chat")).font(.caption).foregroundStyle(.secondary)
				}
				if runtime.composer.state.unknown {
					Button(String(localized: "Review send", table: "Chat")) { graph.navigate(.settings) }.buttonStyle(.glass)
				}
				if let replyID = runtime.composer.state.draft
					.replyID
				{
					HStack {
						Text("Reply to " + (timeline.messages.first { $0.id == replyID }?.content.text ?? "message")).font(.caption)
							.lineLimit(2); Spacer(); Button(
								"Cancel reply",
								systemImage: "xmark",
							) { runtime.composer.send(.reply(nil)) }.labelStyle(.iconOnly).buttonStyle(.glass)
					}
				}
				HStack(alignment: .bottom, spacing: 10) {
					if runtime.attachments.state.limit != nil, allowed {
						Menu { PhotosPicker(
							selection: $photo,
							matching: .any(of: [.images, .videos]),
						) { Label("Photos and videos", systemImage: "photo") }; Button(
							String(localized: "Files", table: "Chat"),
							systemImage: "folder",
						) { filePicker = true
						}
						#if DEBUG
							if ProcessInfo.processInfo
								.environment["XMTP_UI_TEST_ATTACHMENT"] != nil
							{
								Button(String(localized: "Test file", table: "Chat")) { stageTestFile() }
							}
						#endif
						} label: { Image(systemName: "plus").font(.title3) }.buttonStyle(.glass)
							.frame(minWidth: 44, minHeight: 44)
							.glassEffectID("attachment-menu", in: composerNamespace)
							.accessibilityLabel("Add attachment")
					}
					TextField(
						"Message",
						text: Binding(get: { runtime.composer.state.draft.text }, set: { runtime.composer.send(.edit($0)) }),
						axis: .vertical,
					).lineLimit(1 ... 5).padding(.horizontal, 14).padding(.vertical, 11)
						.messengerGlass(in: RoundedRectangle(cornerRadius: 22)).disabled(!runtime.composer.state.enabled)
						.accessibilityIdentifier("message-composer")
					if !runtime.composer.state.draft.text
						.isEmpty
					{
						Button(String(localized: "Send message", table: "Chat"), systemImage: "arrow.up") {
							submittedText = runtime.composer.state.draft.text; runtime.composer.send(.submit)
						}.labelStyle(.iconOnly)
							.buttonStyle(.glassProminent).disabled(!runtime.composer.state.enabled).accessibilityIdentifier("send-message")
					}
				}
			}.anchorPreference(key: MessageFlightAnchors.self, value: .bounds) { ["composer": $0] }.padding(12)
		}
	}

	#if DEBUG
		func stageTestFile() {
			guard let input = ProcessInfo.processInfo.environment["XMTP_UI_TEST_ATTACHMENT"],
			      let separator = input.firstIndex(of: ":"),
			      let bytes = Data(base64Encoded: String(input[input.index(after: separator)...])),
			      bytes.count <= 65536, let limit = runtime.attachments.state.limit,
			      let profile = graph.active?.key.profileID else { return }
			do {
				let name = URL(fileURLWithPath: String(input[..<separator])).lastPathComponent
				let directory = try graph.files.paths(profile).root.appendingPathComponent("temp")
				let url = try FileStager.stage(bytes, directory: directory, limit: limit)
				runtime.attachments.send(.stage(url, name))
			} catch { self.error = "Cannot stage test file." }
		}

	#endif
	func stage(_ source: URL) {
		do { guard let limit = runtime.attachments.state.limit,
		           let profile = graph.active?.key.profileID else { return }; let url = try FileStager.stage(
				source,
				directory: graph.files.paths(profile).root.appendingPathComponent("temp"),
				limit: limit,
			); runtime.attachments.send(.stage(url, source.lastPathComponent))
		} catch FileStager.StagingError
			.tooLarge { self.error = "File exceeds the server upload limit" } catch
		{
			self.error = "Cannot read this file. Try again."
		}
	}
}
