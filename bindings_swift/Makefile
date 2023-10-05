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

all: framework

$(ARCHS_IOS): %:
	cargo build --target $@ --target-dir ./target --release --no-default-features

$(ARCHS_MAC): %:
	cargo build --target $@ --target-dir ./target --release --no-default-features

aarch64-apple-ios:
	cargo build --target $@ --target-dir ./target --release

$(LIB): $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios
	rm -rf lipo_macos lipo_ios_simulator
	mkdir -p lipo_macos lipo_ios_simulator
	lipo -create -output lipo_ios_simulator/libxmtp_rust_swift.a $(foreach arch,$(ARCHS_IOS),$(wildcard target/$(arch)/release/$(LIB)))
	lipo -create -output lipo_macos/libxmtp_rust_swift.a $(foreach arch,$(ARCHS_MAC),$(wildcard target/$(arch)/release/$(LIB)))

move_gen:
	rm -rf Generated
	# Clean and build to regenerate swift-bridge "Generated" directory
	cargo clean
	cargo build
	rm -rf include/Generated
	# Move "Generated" directory to "include"
	cp -r Generated include/
	# Delete the *.swift files within include/Generated but
	# Keep top-level Generated for push-to-swift step where .swift files are
	# copied over to another repo (xmtp-rust-swift)
	find include/Generated -type f -name "*.swift" -delete

framework: move_gen $(LIB)
	rm -rf XMTPRustSwift.xcframework
	xcodebuild -create-xcframework \
		-library ./lipo_macos/libxmtp_rust_swift.a \
		-headers ./include/ \
		-library ./lipo_ios_simulator/libxmtp_rust_swift.a \
		-headers ./include/ \
		-library ./target/aarch64-apple-ios/release/libxmtp_rust_swift.a \
		-headers ./include/ \
		-output XMTPRustSwift.xcframework
	rm -f bundle.zip
	zip -r bundle.zip XMTPRustSwift.xcframework
	openssl dgst -sha256 bundle.zip

swift: framework
	./copy_xcframework_to_swift.sh

.PHONY: $(ARCHS_IOS) $(ARCHS_MAC) framework all aarch64-apple-ios download-toolchains swift
