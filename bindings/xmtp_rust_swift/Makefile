# Simulator config
ARCHS_IOS = x86_64-apple-ios aarch64-apple-ios-sim
ARCHS_MAC = x86_64-apple-darwin aarch64-apple-darwin
# Not used
# ARCHS_MACCATALYST = x86_64-apple-ios-macabi aarch64-apple-ios-macabi
LIB=libxmtp_rust_swift.a

all: pkg

.PHONY: $(ARCHS_IOS) $(ARCHS_MAC) pkg all aarch64-apple-ios
$(ARCHS_IOS): %:
	cargo build --target $@ --release --no-default-features

$(ARCHS_MAC): %:
	cargo build --target $@ --release --no-default-features

aarch64-apple-ios:
	cargo build --target $@ --release

$(LIB): $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios
	lipo -create -output libxmtp_rust_swift_iossimulator.a $(foreach arch,$(ARCHS_IOS),$(wildcard target/$(arch)/release/$(LIB)))
	lipo -create -output libxmtp_rust_swift_macos.a $(foreach arch,$(ARCHS_MAC),$(wildcard target/$(arch)/release/$(LIB)))

pkg: $(LIB)
	xcodebuild -create-xcframework \
		-library ./libxmtp_rust_swift_macos.a \
		-headers ./include/ \
		-library ./libxmtp_rust_swift_iossimulator.a \
		-headers ./include/ \
		-library ./target/aarch64-apple-ios/release/libxmtp_rust_swift.a \
		-headers ./include/ \
		-output XMTPRustSwift.xcframework
