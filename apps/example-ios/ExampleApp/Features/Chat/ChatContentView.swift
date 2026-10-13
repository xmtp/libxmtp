import ExampleCore
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ChatContentView: View {
	let graph: AppGraph; let id: String; let runtime: ChatRuntime
	@Environment(\.scenePhase) var scenePhase
	@Environment(\.accessibilityReduceMotion) var reduceMotion
	@Environment(\.motionPolicy) var motion
	@State var submittedText: String?
	@State var flight: MessageFlight?
	@State var armedReplyID: String?
	@Namespace var composerNamespace
	@State var filePicker = false
	@State var photo: PhotosPickerItem?
	@State var error: String?
	var timeline: ChatTimelineState {
		runtime.timeline.state
	}

	var allowed: Bool {
		timeline.conversation?.consent == .allowed
	}

	var own: String {
		graph.active?.profile.inboxID ?? ""
	}

	var body: some View {
		ScrollViewReader { proxy in
			VStack(spacing: 0) {
				if let notice = timeline.window.notice {
					Text(String(localized: String.LocalizationValue(notice), table: "Chat")).font(.caption).padding(8)
				}
				if let failure = timeline
					.error
				{
					HStack {
						Text(String(localized: String.LocalizationValue(failure), table: "Chat")); Button(String(
							localized: "Retry",
							table: "Chat",
						)) { runtime.timeline.send(.refresh) }
							.buttonStyle(.glass)
					}.font(.caption)
						.padding(8)
				}
				ScrollView {
					LazyVStack(spacing: 10) {
						if timeline.window
							.hasOlder
						{
							Button(String(localized: "Older messages", table: "Chat")) { runtime.timeline.send(.older) }.buttonStyle(.glass)
								.disabled(timeline.loading)
						}
						if timeline
							.morePending
						{
							Button(String(localized: "Older pending messages", table: "Chat")) { runtime.timeline.send(.olderPending) }
								.buttonStyle(.glass); Button(String(localized: "Newest pending messages", table: "Chat")) {
									runtime.timeline.send(.newestPending)
								}
								.buttonStyle(.glass)
						}
						ForEach(Array(timeline.messages.enumerated()), id: \.element.id) { index, row in
							if index == 0 || !Calendar.current.isDate(row.date, inSameDayAs: timeline.messages[index - 1].date) {
								Text(
									row.date,
									format: .dateTime.month().day().hour().minute(),
								).font(.caption2).foregroundStyle(.secondary).padding(.vertical, 12)
							}
							message(row).id(row.id).transition(reduceMotion ? .opacity : .identity)
						}
						Color.clear.frame(height: 1).id("latest")
					}.scrollTargetLayout().padding(.horizontal, 16).padding(.bottom, 16)
						.animation(motion.animation, value: timeline.messages.map(\.id))
				}.defaultScrollAnchor(.bottom)
					.onScrollTargetVisibilityChange(idType: String.self) { ids in runtime.timeline.send(.visibleMessages(
						ids,
						foreground: scenePhase == .active,
					)) }
					.onChange(of: timeline.messages.last?.id) { _, _ in
						if timeline
							.atNewest
						{
							withAnimation(reduceMotion ? .linear(duration: 0.2) : .snappy(duration: 0.22)) { proxy.scrollTo(
								"latest",
								anchor: .bottom,
							) }
						}
					}
					.onChange(of: timeline.target) { _, target in
						if let target {
							proxy.scrollTo(target, anchor: .center)
						}
					}
				if !timeline.window
					.atNewest
				{
					Button(String(localized: "Jump to latest", table: "Chat"), systemImage: "arrow.down") {
						runtime.timeline.send(.newest); proxy.scrollTo(
							"latest",
							anchor: .bottom,
						)
					}.buttonStyle(.glass).padding(8)
				}
				if timeline.conversation?.consent == .unknown {
					requestBar.transition(motion.transition)
				} else {
					composer
				}
			}
		}.overlayPreferenceValue(MessageFlightAnchors.self) { anchors in
			GeometryReader { proxy in
				if let flight, let source = anchors["composer"], let target = anchors[flight.id] {
					MessageFlightView(text: flight.text, source: proxy[source], target: proxy[target], motion: motion) {
						self.flight = nil
					}
				}
			}.allowsHitTesting(false).accessibilityHidden(true)
		}
		.animation(motion.animation, value: allowed)
		.animation(motion.animation, value: runtime.attachments.state.drafts.map(\.id))
		.onChange(of: timeline.messages.last?.id) { _, _ in
			guard let text = submittedText, let row = timeline.messages.last, row.sender == own,
			      row.content.text == text else { return }
			submittedText = nil
			if !reduceMotion {
				flight = MessageFlight(id: row.id, text: text)
			}
		}
		.background(Color(uiColor: .systemBackground))
		.navigationTitle(timeline.conversation?.title(alias: graph.preferences?.alias(for: id)) ?? "Conversation")
		.navigationBarTitleDisplayMode(.inline)
		.toolbar {
			ToolbarItem(placement: .principal) {
				Button { graph.navigate(.details(id)) } label: {
					let title = timeline.conversation?.title(alias: graph.preferences?.alias(for: id)) ?? "Conversation"
					let initials = timeline.conversation
						.map { ConversationRow(conversation: $0, title: title, muted: false).initials } ?? ""
					ChatHeader(id: id, title: title, initials: initials, memberCount: timeline.conversation?.memberCount)
				}
				.buttonStyle(.plain).accessibilityIdentifier("chat.details")
			}
		}
		.onChange(of: allowed, initial: true) { _, value in
			runtime.composer.send(.allow(value)); runtime.actions.allowed = value
		}
		.onChange(of: graph.requests?.state.busy) { _, busy in
			if busy == false {
				runtime.timeline.send(.refresh)
			}
		}
		.task(id: timeline.messages.map(\.sender)) {
			do { try await graph.identityDirectory?.load(Array(Set(timeline.messages.map(\.sender)))) } catch {}
		}
		.alert(
			"Delete message?",
			isPresented: Binding(
				get: { runtime.actions.state.deleteTarget != nil },
				set: {
					if !$0 {
						runtime.actions.send(.cancelDelete)
					}
				},
			),
		) {
			Button(String(localized: "Delete message", table: "Chat"), role: .destructive) {
				runtime.actions.send(.confirmDelete)
			}; Button(
				"Cancel",
				role: .cancel,
			) {}
		} message: { Text(
			"This sends a deletion request to this conversation. Other members may already have a copy.",
			tableName: "Chat",
		)
		}
		.fileImporter(isPresented: $filePicker, allowedContentTypes: [.item]) { result in
			switch result {
			case let .success(url): stage(url)
			case .failure: error = "Cannot select this file."
			}
		}
		.onChange(of: photo) { _, value in
			guard let value, let limit = runtime.attachments.state.limit,
			      let profile = graph.active?.key.profileID else { return }
			do {
				let directory = try graph.files.paths(profile).root.appendingPathComponent("temp")
				runtime.imports.start(value, directory: directory, limit: limit, callbacks: .init(
					admitted: { graph.routes.admit(runtime.origin, fence: graph.fence) },
					receive: { file in
						let name = URL(fileURLWithPath: file.filename).pathExtension.isEmpty
							? "Photo." + (value.supportedContentTypes.compactMap(\.preferredFilenameExtension).first ?? "jpg") : file
							.filename
						runtime.attachments.send(.stage(file.url, name))
					}, failure: { _ in error = "Cannot select this photo." },
				))
			} catch { self.error = "Cannot select this photo." }
		}
	}
}
