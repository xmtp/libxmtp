PROJECT_NAME = xmtpv3
WORKSPACE_MANIFEST=$(shell cargo locate-project --workspace --message-format=plain)
WORKSPACE_PATH=$(shell dirname $(WORKSPACE_MANIFEST))
TARGET_DIR=$(shell cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')

# Simulator config (Apple Silicon only - Intel simulator dropped for Nix compatibility)
# IMPROVEMENT: x86_64-apple-ios-sim could be restored if needed for Rosetta-based
# simulator testing. Would require adding it to iosTargets in ios-env.nix too.
ARCHS_IOS = aarch64-apple-ios-sim
ARCHS_MAC = x86_64-apple-darwin aarch64-apple-darwin
LIB=libxmtpv3.a
DYLIB=libxmtpv3.dylib
JAR_DIR=$(shell pwd)/tests/jar
SQLCIPHER_DIR=$(shell pwd)/sqlcipher
SQLCIPHER_LIB=$(SQLCIPHER_DIR)/.libs/libsqlcipher.a
GIT_BRANCH=$(shell git rev-parse --abbrev-ref HEAD)
GIT_COMMIT_HASH=$(shell git log -1 --pretty=format:"%h")
GIT_COMMIT_DATE=$(shell TZ=UTC git log -1 --date=iso-local --pretty=format:"%ad")

IOS_SDK_BUILD_DIR ?= $(WORKSPACE_PATH)/sdks/ios/.build
IOS_SDK_SWIFT_DIR ?= $(WORKSPACE_PATH)/sdks/ios/Sources/XMTPiOS/LibXMTP

NIX_OUT ?= build/nix
SWIFT_OUT ?= $(NIX_OUT)/swift

install-jar:
	mkdir -p $(JAR_DIR) && \
	curl https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar -o $(JAR_DIR)/jna.jar && \
	curl https://repo1.maven.org/maven2/org/jetbrains/kotlinx/kotlinx-coroutines-core-jvm/1.7.3/kotlinx-coroutines-core-jvm-1.7.3.jar -o $(JAR_DIR)/kotlinx-coroutines-core-jvm.jar && \
	curl https://repo1.maven.org/maven2/org/web3j/crypto/5.0.0/crypto-5.0.0.jar -o $(JAR_DIR)/web3j-crypto.jar && \
	curl https://repo1.maven.org/maven2/org/web3j/utils/5.0.0/utils-5.0.0.jar -o $(JAR_DIR)/web3j-utils.jar && \
	curl https://repo1.maven.org/maven2/org/bouncycastle/bcprov-jdk15on/1.70/bcprov-jdk15on-1.70.jar -o $(JAR_DIR)/bouncycastle.jar && \
	$(MAKE) echo-jar

echo-jar:
	echo "\nAdd the following line to your .zshrc:\nexport CLASSPATH=\"$(shell echo $(JAR_DIR)/*.jar | sed -e 's/ /:/g')\""

download-toolchains:
	rustup target add $(ARCHS_IOS)
	rustup target add $(ARCHS_MAC)
	rustup target add aarch64-apple-ios

all: framework

libxmtp-version:
	echo "Version: $(GIT_COMMIT_HASH)\nBranch: $(GIT_BRANCH)\nDate: $(GIT_COMMIT_DATE)" > libxmtp-version.txt

$(ARCHS_IOS): %:
	IPHONEOS_DEPLOYMENT_TARGET=14 cargo build --target $@ --target-dir $(TARGET_DIR) --release --no-default-features
	mkdir -p build/$@
	mv $(TARGET_DIR)/$@/release/$(LIB) build/$@/$(LIB)
	mv $(TARGET_DIR)/$@/release/$(DYLIB) build/$@/$(DYLIB)

$(ARCHS_MAC): %:
	cargo build --target $@ --target-dir $(TARGET_DIR) --release --no-default-features
	mkdir -p build/$@
	mv $(TARGET_DIR)/$@/release/$(LIB) build/$@/$(LIB)
	mv $(TARGET_DIR)/$@/release/$(DYLIB) build/$@/$(DYLIB)

aarch64-apple-ios:
	IPHONEOS_DEPLOYMENT_TARGET=14 cargo build --target $@ --target-dir $(TARGET_DIR) --release
	mkdir -p build/$@
	mv $(TARGET_DIR)/$@/release/$(LIB) build/$@/$(LIB)
	mv $(TARGET_DIR)/$@/release/$(DYLIB) build/$@/$(DYLIB)

$(LIB): $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios
$(DYLIB): $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios

# lipo combines libs for different architectures (aarch64, x86_64, ...) into one fat lib
lipo:
	mkdir -p build/lipo_macos build/lipo_ios_sim
	lipo -create -output build/lipo_ios_sim/$(LIB) $(foreach arch,$(ARCHS_IOS),$(wildcard $(NIX_OUT)/$(arch)/$(LIB)))
	lipo -create -output build/lipo_macos/$(LIB) $(foreach arch,$(ARCHS_MAC),$(wildcard $(NIX_OUT)/$(arch)/$(LIB)))

lipodyn:
	mkdir -p build/lipo_macos build/lipo_ios_sim
	lipo -create -output build/lipo_ios_sim/$(DYLIB) $(foreach arch,$(ARCHS_IOS),$(wildcard $(NIX_OUT)/$(arch)/$(DYLIB)))
	lipo -create -output build/lipo_macos/$(DYLIB) $(foreach arch,$(ARCHS_MAC),$(wildcard $(NIX_OUT)/$(arch)/$(DYLIB)))
	install_name_tool -id @rpath/libxmtpv3.dylib build/lipo_ios_sim/$(DYLIB)
	install_name_tool -id @rpath/libxmtpv3.dylib build/lipo_macos/$(DYLIB)

# xcframework combines libs for different platforms (iOS, iOS-simulator, macOS, ...) into one framework that can be used in Xcode
framework: lipo
	mkdir -p build/swift
	mkdir -p $(IOS_SDK_SWIFT_DIR)
	chmod -R u+w build/swift/LibXMTPSwiftFFI.xcframework 2>/dev/null || true
	rm -rf build/swift/LibXMTPSwiftFFI.xcframework
	xcodebuild -create-xcframework \
		-library $(NIX_OUT)/aarch64-apple-ios/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-library build/lipo_ios_sim/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-library build/lipo_macos/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-output build/swift/LibXMTPSwiftFFI.xcframework
	cp -f $(SWIFT_OUT)/xmtpv3.swift $(IOS_SDK_SWIFT_DIR)/xmtpv3.swift

# Fast xcframework: simulator + macOS only, no lipo needed (single arch each), static only.
# Used by sdks/ios/dev/bindings (default) for fast local dev builds.
framework-fast:
	mkdir -p build/swift
	mkdir -p $(IOS_SDK_SWIFT_DIR)
	chmod -R u+w build/swift/LibXMTPSwiftFFI.xcframework 2>/dev/null || true
	rm -rf build/swift/LibXMTPSwiftFFI.xcframework
	xcodebuild -create-xcframework \
		-library $(NIX_OUT)/aarch64-apple-ios-sim/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-library $(NIX_OUT)/aarch64-apple-darwin/$(LIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-output build/swift/LibXMTPSwiftFFI.xcframework
	cp -f $(SWIFT_OUT)/xmtpv3.swift $(IOS_SDK_SWIFT_DIR)/xmtpv3.swift

frameworkdyn: lipodyn
	mkdir -p build/swift
	chmod -R u+w build/swift/LibXMTPSwiftFFIDynamic.xcframework 2>/dev/null || true
	rm -rf build/swift/LibXMTPSwiftFFIDynamic.xcframework
	xcodebuild -create-xcframework \
		-library $(NIX_OUT)/aarch64-apple-ios/$(DYLIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-library build/lipo_ios_sim/$(DYLIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-library build/lipo_macos/$(DYLIB) \
		-headers $(SWIFT_OUT)/include/libxmtp/ \
		-output build/swift/LibXMTPSwiftFFIDynamic.xcframework

bindgenstatic:
	cargo build --release -p xmtpv3
	rm -rf build/swift/static
	cargo run --bin ffi-uniffi-bindgen --release --features uniffi/cli generate \
		--library $(TARGET_DIR)/release/$(LIB) \
		--out-dir build/swift/static \
		--language swift

swift: libxmtp-version
	# https://mozilla.github.io/uniffi-rs/swift/module.html#compiling-a-swift-module
	mkdir -p build/swift/static/include/libxmtp
	mv build/swift/static/$(PROJECT_NAME)FFI.h build/swift/static/include/libxmtp/  # Move header
	mv build/swift/static/$(PROJECT_NAME)FFI.modulemap build/swift/static/include/libxmtp/module.modulemap  # Move modulemap
	cp libxmtp-version.txt build/swift/static

swiftlocal: libxmtpv3.a bindgenstatic swift framework

# Build everything needed for local iOS SDK development.
# Uses cargo directly (no nix build) — for devs who want a self-contained build
# without Nix. The nix build path (`nix build .#ios-libs` + `make framework`)
# is preferred for CI and when Cachix caching is available.
# IMPROVEMENT: Document when to use `make local` vs `nix build .#ios-libs && make framework`
# in a CONTRIBUTING or README section for iOS developers.
local: NIX_OUT = build
local: SWIFT_OUT = build/swift/static
local: $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios bindgenstatic swift lipo framework lipodyn frameworkdyn

.PHONY: $(ARCHS_IOS) $(ARCHS_MAC) framework framework-fast all aarch64-apple-ios install-jar echo-jar download-toolchains swift bindgenstatic frameworkdyn lipo lipodyn local
