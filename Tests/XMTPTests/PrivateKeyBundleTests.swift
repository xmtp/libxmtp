//
//  PrivateKeyBundleTests.swift
//
//
//  Created by Pat Nakajima on 11/29/22.
//

import secp256k1
import XCTest
@testable import XMTP

class PrivateKeyBundleTests: XCTestCase {
	func testConversion() async throws {
		let wallet = try PrivateKey.generate()
		let v1 = try await PrivateKeyBundleV1.generate(wallet: wallet)

		let v2 = try v1.toV2()

		let v2PreKeyPublic = try UnsignedPublicKey(serializedData: v2.preKeys[0].publicKey.keyBytes)
		XCTAssertEqual(v1.preKeys[0].publicKey.secp256K1Uncompressed.bytes, v2PreKeyPublic.secp256K1Uncompressed.bytes)
	}

	func testKeyBundlesAreSigned() async throws {
		let wallet = try PrivateKey.generate()
		let v1 = try await PrivateKeyBundleV1.generate(wallet: wallet)

		XCTAssert(v1.identityKey.publicKey.hasSignature, "no private v1 identity key signature")
		XCTAssert(v1.preKeys[0].publicKey.hasSignature, "no private v1 pre key signature")
		XCTAssert(v1.toPublicKeyBundle().identityKey.hasSignature, "no public v1 identity key signature")
		XCTAssert(v1.toPublicKeyBundle().preKey.hasSignature, "no public v1 pre key signature")

		let v2 = try v1.toV2()
		XCTAssert(v2.identityKey.publicKey.hasSignature, "no private v2 identity key signature")
		XCTAssert(v2.preKeys[0].publicKey.hasSignature, "no private v2 pre key signature")
		XCTAssert(v2.getPublicKeyBundle().identityKey.hasSignature, "no public v2 identity key signature")
		XCTAssert(v2.getPublicKeyBundle().preKey.hasSignature, "no public v2 pre key signature")
	}

	func testSharedSecret() async throws {
		let alice = try PrivateKey.generate()
		let alicePrivateBundle = try await PrivateKeyBundleV1.generate(wallet: alice).toV2()
		let alicePublicBundle = alicePrivateBundle.getPublicKeyBundle()

		let bob = try PrivateKey.generate()
		let bobPrivateBundle = try await PrivateKeyBundleV1.generate(wallet: bob).toV2()
		let bobPublicBundle = bobPrivateBundle.getPublicKeyBundle()

		let aliceSharedSecret = try alicePrivateBundle.sharedSecret(peer: bobPublicBundle, myPreKey: alicePublicBundle.preKey, isRecipient: true)

		let bobSharedSecret = try bobPrivateBundle.sharedSecret(peer: alicePublicBundle, myPreKey: bobPublicBundle.preKey, isRecipient: false)

		XCTAssertEqual(aliceSharedSecret, bobSharedSecret)
	}

	func testSharedSecretMatchesWhatJSGenerates() throws {
		let meBundleData = Data("0a86030ac00108a687b5d8cc3012220a20db73e1b4b5aeffb6cecd37526d842327730433e1751bceb5824d937f779797541a920108a687b5d8cc3012440a420a40d35c081d9ab59b3fb13e27cb03a225c7134bc4ce4ce51f80273481c31d803e1e4fa8ae43e7ec20b06a81b694ad28470f85fc971b8050867f5a4821c03a67f0e81a430a410443631548a55a60f06989ce1bc3fa43fdbe463ea4748dcb509e09fc58514c6e56edfac83e1fff5f382bc110fa066762f4b862db8df53be7d48268b3fdf649adc812c00108b787b5d8cc3012220a209e2631f34af8fc1ec0f75bd15ee4e110ac424300f39bff26c7a990a75a49ac641a920108b787b5d8cc3012440a420a40202a68a2e95d446511ecf22f5487b998989989adfc0a60e1ce201e0bab64d836066ccda987cda99c0e588babb8c334a820d6a6e360100ba7ba08e0e339a303681a430a4104c9733798111d89446264db365bc0dde54b5f9202eeb309eec2f18c572ce11e267fe91e184207676d7af5eaf2ad65de0881093623030f6096ea5bf3ecd252c482".web3.bytesFromHex!)

		let youBundleData = Data("0a940108c487b5d8cc3012460a440a40c51e611e662117991b19f60b6a7f6d9f08671c3d55241e959954c2e0f2ec47d15b872986d2a279ffe55df01709b000fbdcc9e85c1946876e187f90a0fd32222c10011a430a41049cccf02f766f7d4c322eeb498f2ac0283a011992fc77f9e0d5687b826aafd48d8319f48f773ec959221bf7bf7d3da4b09e59af540a633c588df2f1b6f465d6a712940108cb87b5d8cc3012460a440a40b7b0e89ce4789f6e78502357864979abe9e26cd44a36ed75578368a02cdc3bda7d56721660cb2066b76a4a6dd5a78d99df4b096cc4622a2065cf05b2f32b94be10011a430a410438f2b23a4e0f9c61e716b8cf4b23f2709d92b4feb71429a385b6878c31085384701bc787def9396b441bfb8751c042432785c352f8ee9bfb9c6cd5d6871b2d1a".web3.bytesFromHex!)

		let secretData = Data("049f4cd17426f9dfac528f400db858a9cbc87488879d6df5bea3595beaeb37415f1b24227e571dd4969406f366841e682795f284b54952a22b2dcff87971580fa604c0a97d550ce3ce5dac2e5469a2e3ece7232d80247a789044ebef0478c6911d63400a13090de6e8aeb4a1bcb878ca73b1d7eb13ab3012e564cfef74a8182467cc047d999bb077e5b223509fab7a08642c29359b8c3144ffa30002e45f09e4a515927f682eb71b68bd52f498d5d464c6bb14d3c07aefc86a1ab8e2528a21ffd41912".web3.bytesFromHex!)

		let meBundle = try PrivateKeyBundle(serializedData: meBundleData).v1.toV2()
		let youBundlePublic = try SignedPublicKeyBundle(try PublicKeyBundle(serializedData: youBundleData))

		let secret = try meBundle.sharedSecret(peer: youBundlePublic, myPreKey: meBundle.preKeys[0].publicKey, isRecipient: true)

		XCTAssertEqual(secretData, secret)
	}
}
