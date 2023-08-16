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

# lib: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios
lib:
	for arch in $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios; do \
	    mkdir -p build/$$arch/; \
		cp target/$$arch/release/$(LIB) build/$$arch/; \
	done

lipo:
	rm -rf build/lipo_macos build/lipo_ios_simulator
	mkdir -p build/lipo_macos build/lipo_ios_simulator
	lipo -create -output build/lipo_ios_simulator/$(LIB) $(foreach arch,$(ARCHS_IOS),$(wildcard build/$(arch)/$(LIB)))
	lipo -create -output build/lipo_macos/$(LIB) $(foreach arch,$(ARCHS_MAC),$(wildcard build/$(arch)/$(LIB)))

bridge:
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
	./make_xmtp_rust_swift_sources.sh

framework: lipo
	rm -rf XMTPRustSwift.xcframework
	xcodebuild -create-xcframework \
		-library build/lipo_macos/libxmtp_rust_swift.a -headers include/ \
		-library build/lipo_ios_simulator/libxmtp_rust_swift.a -headers include/ \
		-library build/aarch64-apple-ios/libxmtp_rust_swift.a -headers include/ \
		-output XMTPRustSwift.xcframework

swift: framework
	rm -f xmtp-rust-swift.zip
	zip -r xmtp-rust-swift.zip XMTPRustSwift.xcframework Sources
	openssl dgst -sha256 xmtp-rust-swift.zip

.PHONY: $(ARCHS_IOS) $(ARCHS_MAC) framework all aarch64-apple-ios download-toolchains swift
