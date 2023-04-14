# Simulator config
ARCHS_IOS = x86_64-apple-ios aarch64-apple-ios-sim
ARCHS_MAC = x86_64-apple-darwin aarch64-apple-darwin
# Not used
# ARCHS_MACCATALYST = x86_64-apple-ios-macabi aarch64-apple-ios-macabi
LIB=libxmtp_rust_swift.a

download-toolchains:
	rustup target add $(ARCHS_IOS)
	rustup target add $(ARCHS_MAC)
	rustup target add aarch64-apple-ios

all: pkg

$(ARCHS_IOS): %:
	cargo build --target $@ --release --no-default-features

$(ARCHS_MAC): %:
	cargo build --target $@ --release --no-default-features

aarch64-apple-ios:
	cargo build --target $@ --release

$(LIB): $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios
	rm -f libxmtp_rust_swift_iossimulator.a libxmtp_rust_swift_macos.a
	lipo -create -output libxmtp_rust_swift_iossimulator.a $(foreach arch,$(ARCHS_IOS),$(wildcard target/$(arch)/release/$(LIB)))
	lipo -create -output libxmtp_rust_swift_macos.a $(foreach arch,$(ARCHS_MAC),$(wildcard target/$(arch)/release/$(LIB)))

move_gen:
	# Move "Generated" directory to "include"
	rm -rf include/Generated
	cp -r Generated include/

pkg: $(LIB) move_gen
	rm -rf XMTPRustSwift.xcframework
	xcodebuild -create-xcframework \
		-library ./libxmtp_rust_swift_macos.a \
		-headers ./include/ \
		-library ./libxmtp_rust_swift_iossimulator.a \
		-headers ./include/ \
		-library ./target/aarch64-apple-ios/release/libxmtp_rust_swift.a \
		-headers ./include/ \
		-output XMTPRustSwift.xcframework
	rm -f bundle.zip
	zip -r bundle.zip XMTPRustSwift.xcframework
	openssl dgst -sha256 bundle.zip

push-to-derived:
	./copy_xcframework_to_deriveddata.sh

push-to-swift:
	./copy_xcframework_to_swift.sh

.PHONY: $(ARCHS_IOS) $(ARCHS_MAC) pkg all aarch64-apple-ios push-to-derived download-toolchains
