import ExampleCore
import SwiftUI

struct HandleView: View {
	let store: HandleStore
	@ScaledMetric(relativeTo: .largeTitle) private var heroTitleSize = 32
	@ScaledMetric(relativeTo: .largeTitle) private var heroSize = 72
	@ScaledMetric(relativeTo: .subheadline) private var fieldLabelSize = 14
	var body: some View {
		Group {
			if store.state.mode == .choose {
				chooseContent
			} else {
				changeContent
			}
		}
		.safeAreaInset(edge: .bottom, spacing: 0) {
			ActionFooter {
				Button { store.send(store.state.canClaim ? .claim : .check) } label: { Text(primaryLabel).frame(maxWidth: .infinity)
				}.buttonStyle(.glassProminent).controlSize(.large)
					.disabled(!store.state.canClaim && !store.state.canCheck); if store
					.state.mode == .choose
				{
					Button(String(
						localized: "Skip for now",
						table: "ChooseHandle",
					)) { store.send(.skip) }.buttonStyle(.plain).font(.headline).foregroundStyle(Theme.tint)
						.frame(minHeight: 44).disabled(store.state.claiming != nil)
				}
			}
		}
		.navigationTitle(store.state.mode == .choose ? "" : "Change handle")
		.task { store.send(.load) }
		.alert(
			store.state.current == nil ? "Claim this handle?" : "Change your handle?",
			isPresented: Binding(get: { store.state.confirm != nil }, set: {
				if !$0 {
					store.send(.cancelClaim)
				}
			}),
		) {
			Button(store.state.current == nil ? "Claim" : "Change") { store.send(.confirmClaim) }; Button(
				String(localized: "Cancel", table: "ChooseHandle"),
				role: .cancel,
			) { store.send(.cancelClaim) }
		} message: {
			Text("People will see \(store.state.confirm ?? "") as your handle. You cannot change your handle again for 7 days.")
		}
	}
}

private extension HandleView {
	var chooseContent: some View {
		ScrollView {
			VStack(spacing: 30) {
				VStack(spacing: 20) {
					Image(systemName: "at")
						.font(.system(size: heroSize * 0.55))
						.foregroundStyle(.white)
						.frame(width: heroSize, height: heroSize)
						.background(Theme.tint, in: .rect(cornerRadius: heroSize / 3.4))
					VStack(spacing: 12) {
						Text(String(localized: "Choose your handle.", table: "ChooseHandle"))
							.font(.system(size: heroTitleSize, weight: .bold))
						Text(String(
							localized: "Your handle is public. People on this server use it to find you and send you messages.",
							table: "ChooseHandle",
						))
						.font(.callout)
						.foregroundStyle(Theme.secondaryLabel)
						.multilineTextAlignment(.center)
					}
				}
				.frame(maxWidth: .infinity)
				VStack(alignment: .leading, spacing: 0) {
					Text(String(localized: "Your handle", table: "ChooseHandle"))
						.font(.system(size: fieldLabelSize, weight: .semibold))
						.foregroundStyle(Theme.secondaryLabel)
						.padding(.horizontal, 16).padding(.bottom, 5)
					TextField("nick", text: Binding(
						get: { store.state.input }, set: { store.send(.changeInput($0)) },
					))
					.textInputAutocapitalization(.never).autocorrectionDisabled()
					.disabled(!store.state.editable)
					.padding(.horizontal, 16).frame(minHeight: 52)
					.background(Color(.secondarySystemGroupedBackground), in: .capsule)
					chooseInputHint.padding(.horizontal, 16).padding(.top, 10)
					if store.state.loading || store.state.status != .editing || store.state.notice != nil {
						chooseFeedback.padding(.top, 10)
					}
					Text(String(localized: "You can change your handle once every 7 days.", table: "ChooseHandle"))
						.font(.footnote).foregroundStyle(Theme.secondaryLabel)
						.padding(.horizontal, 16).padding(.top, 24)
				}
			}
			.padding(.horizontal, 16).padding(.top, 40).padding(.bottom, 24)
		}
		.background(Color(.systemGroupedBackground))
		.toolbar(.hidden, for: .navigationBar)
	}

	@ViewBuilder private var chooseInputHint: some View {
		if let problem = store.state.problem {
			Text(HandleCopy.problem(problem, input: store.state.input, minimum: store.state.minimum))
				.font(.footnote).foregroundStyle(.red)
		} else {
			Text("\(store.state.minimum) to 27 lowercase letters, numbers, _ or -.")
				.font(.footnote).foregroundStyle(Theme.secondaryLabel)
		}
	}

	@ViewBuilder private var chooseFeedback: some View {
		if store.state.status == .checking || store.state.loading {
			ProgressView().padding(.horizontal, 16)
		}
		if case let .suggestions(input, _, _, _) = store.state.status {
			Text("\(input) is taken. These handles are free:").font(.subheadline).padding(.horizontal, 16)
			VStack(spacing: 0) {
				ForEach(store.state.status.visible, id: \.self) { handle in
					Button { store.send(.select(handle)) } label: {
						HStack {
							HandleText(handle: handle)
							Spacer()
							if store.state.status.pick == handle {
								Image(systemName: "checkmark")
							}
						}.padding(16).frame(maxWidth: .infinity, alignment: .leading)
					}.buttonStyle(.plain)
					Divider().padding(.leading, 16)
				}
				if store.state.status.more {
					Button(String(localized: "Show more", table: "ChooseHandle")) { store.send(.showMore) }
						.frame(maxWidth: .infinity, alignment: .leading).padding(16)
				}
			}.background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 20))
		}
		if case let .available(handle) = store.state.status {
			Text("\(handle) is available.").font(.subheadline).foregroundStyle(.green).padding(.horizontal, 16)
		}
		if case let .noneFree(input) = store.state.status {
			Text("\(input) and the codes we checked are all taken. Look again, or try a different name.")
				.font(.subheadline).padding(.horizontal, 16)
			Button(String(localized: "Look again", table: "ChooseHandle")) { store.send(.check) }
				.padding(.horizontal, 16)
		}
		if let notice = store.state.notice {
			Text(HandleCopy.notice(notice)).font(.subheadline).foregroundStyle(.red).padding(.horizontal, 16)
			if canRetry {
				Button(String(localized: "Retry", table: "ChooseHandle")) { store.send(.retry) }
					.padding(.horizontal, 16)
			}
		}
	}

	var canRetry: Bool {
		switch store.state.notice {
		case .recordFailed, .claimFailed, .checkFailed: true
		default: false
		}
	}

	var changeContent: some View {
		Form {
			if let current = store.state
				.current
			{
				Section(String(localized: "Current handle", table: "ChooseHandle")) {
					HandleText(handle: current); if let since = store.state.currentSince {
						Text("Yours since " + since.formatted(
							date: .abbreviated,
							time: .omitted,
						)).foregroundStyle(Theme.secondaryLabel)
					}
				}
			}
			Section("New handle") {
				TextField("nick", text: Binding(get: { store.state.input }, set: { store.send(.changeInput($0)) }))
					.textInputAutocapitalization(.never).autocorrectionDisabled().disabled(!store.state.editable)
				if let problem = store.state.problem {
					Text(HandleCopy.problem(
						problem,
						input: store.state.input,
						minimum: store.state.minimum,
					)).foregroundStyle(.red)
				} else {
					Text("\(store.state.minimum) to 27 lowercase letters, numbers, _ or -.").foregroundStyle(Theme.secondaryLabel)
				}
				if store.state.status == .checking || store.state.loading {
					ProgressView()
				}
				if case let .suggestions(input, _, _, _) = store.state
					.status
				{
					Text("\(input) is taken. These handles are free:").font(.subheadline)
				}
				if case let .available(handle) = store.state.status {
					Text("\(handle) is available.").foregroundStyle(.green)
				}
				ForEach(store.state.status.visible, id: \.self) { handle in
					Button { store.send(.select(handle)) } label: {
						HStack {
							HandleText(handle: handle); Spacer(); if store.state.status.pick == handle {
								Image(systemName: "checkmark")
							}
						}
					}
				}
				if store.state.status
					.more
				{
					Button(String(localized: "Show more", table: "ChooseHandle")) { store.send(.showMore) }
				}
				if case let .noneFree(input) = store.state
					.status
				{
					Text("\(input) and the codes we checked are all taken. Look again, or try a different name."); Button(String(
						localized: "Look again",
						table: "ChooseHandle",
					)) { store.send(.check) }
				}
				if let notice = store.state
					.notice
				{
					Text(HandleCopy.notice(notice)).foregroundStyle(.red); if case .recordFailed = notice {
						Button(String(
							localized: "Retry",
							table: "ChooseHandle",
						)) { store.send(.retry) }
					}; if case .claimFailed = notice {
						Button(String(
							localized: "Retry",
							table: "ChooseHandle",
						)) { store.send(.retry) }
					}; if notice ==
						.checkFailed
					{
						Button(String(localized: "Retry", table: "ChooseHandle")) { store.send(.retry) }
					}
				}
			}
			if !store.state.others.isEmpty {
				Section { Text(String(
					localized: "They are still yours, and people can still find you by them. Switching back counts as a change.",
					table: "ChooseHandle",
				)); ForEach(store.state.others, id: \.self) { handle in HStack { HandleText(handle: handle); Spacer(); Button(
					String(
						localized: "Use",
						table: "ChooseHandle",
					),
				) { store.send(.switchBack(handle)) }.disabled(!store.state.editable) } } } header: { Text(String(
					localized: "Your other handles",
					table: "ChooseHandle",
				)) }
			}
			Section {
				Text("You can change your handle once every 7 days, up to 64 times in total.")
			}
		}
	}

	var primaryLabel: String {
		if let claiming = store.state.claiming {
			return "Claiming " + claiming + "…"
		}; if let pick = store.state.status
			.pick
		{
			return (store.state.mode == .change ? "Change to " : "Claim ") + pick
		}; return "Continue"
	}
}
