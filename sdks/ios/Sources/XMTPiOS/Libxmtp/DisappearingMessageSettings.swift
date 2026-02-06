//
//  DisappearingMessageSettings.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 2/9/25.
//

public struct DisappearingMessageSettings {
	public let disappearStartingAtNs: Int64
	public let retentionDurationInNs: Int64

	public init(disappearStartingAtNs: Int64, retentionDurationInNs: Int64) {
		self.disappearStartingAtNs = disappearStartingAtNs
		self.retentionDurationInNs = retentionDurationInNs
	}

	static func createFromFfi(_ ffiSettings: FfiMessageDisappearingSettings) -> DisappearingMessageSettings {
		DisappearingMessageSettings(
			disappearStartingAtNs: ffiSettings.fromNs,
			retentionDurationInNs: ffiSettings.inNs,
		)
	}
}
