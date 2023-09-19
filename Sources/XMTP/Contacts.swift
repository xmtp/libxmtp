//
//  Contacts.swift
//
//
//  Created by Pat Nakajima on 12/8/22.
//

import Foundation

/// Provides access to contact bundles.
public actor Contacts {
	var client: Client

	// Save all bundles here
	var knownBundles: [String: ContactBundle] = [:]

	// Whether or not we have sent invite/intro to this contact
	var hasIntroduced: [String: Bool] = [:]

	init(client: Client) {
		self.client = client
	}

	func markIntroduced(_ peerAddress: String, _ isIntroduced: Bool) {
		hasIntroduced[peerAddress] = isIntroduced
	}

	func has(_ peerAddress: String) -> Bool {
		return knownBundles[peerAddress] != nil
	}

	func needsIntroduction(_ peerAddress: String) -> Bool {
		return hasIntroduced[peerAddress] != true
	}

	func find(_ peerAddress: String) async throws -> ContactBundle? {
		if let knownBundle = knownBundles[peerAddress] {
			return knownBundle
		}

		let response = try await client.query(topic: .contact(peerAddress))

		for envelope in response.envelopes {
			// swiftlint:disable no_optional_try
			if let contactBundle = try? ContactBundle.from(envelope: envelope) {
				knownBundles[peerAddress] = contactBundle

				return contactBundle
			}
			// swiftlint:enable no_optional_try
		}

		return nil
	}
}
