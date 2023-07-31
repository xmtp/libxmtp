PROJECT_NAME = xmtpv3

# Simulator config
ARCHS_IOS = x86_64-apple-ios aarch64-apple-ios-sim
ARCHS_MAC = x86_64-apple-darwin aarch64-apple-darwin
# Not used
# ARCHS_MACCATALYST = x86_64-apple-ios-macabi aarch64-apple-ios-macabi
LIB=libbindings_ffi.dylib
JAR_DIR=$(shell pwd)/tests/jar

install-jar:
	mkdir -p $(JAR_DIR) && \
	curl https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.8.0/jna-5.8.0.jar -o $(JAR_DIR)/jna.jar && \
	curl https://repo1.maven.org/maven2/org/jetbrains/kotlinx/kotlinx-coroutines-core-jvm/1.6.4/kotlinx-coroutines-core-jvm-1.6.4.jar -o $(JAR_DIR)/kotlinx-coroutines-core-jvm.jar && \
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

$(ARCHS_IOS): %:
	cargo build --target $@ --target-dir ./target --release --no-default-features

$(ARCHS_MAC): %:
	cargo build --target $@ --target-dir ./target --release --no-default-features

aarch64-apple-ios:
	cargo build --target $@ --target-dir ./target --release

$(LIB): $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios

framework: swift $(LIB)
	rm -rf LibXMTPRust.xcframework
	xcodebuild -create-xcframework \
		-library ./target/aarch64-apple-ios/release/$(LIB) \
		-headers ./swift/include/ \
		-library ./target/aarch64-apple-ios-sim/release/$(LIB) \
		-headers ./swift/include/ \
		-output LibXMTPRust.xcframework

swift:
	cargo build --release
	rm -rf swift
	target/release/ffi-uniffi-bindgen generate \
		--lib-file target/release/$(LIB) \
		src/$(PROJECT_NAME).udl \
		--out-dir swift \
		--language swift
	# https://mozilla.github.io/uniffi-rs/swift/module.html#compiling-a-swift-module
	mkdir -p swift/include
	mv swift/$(PROJECT_NAME)FFI.h swift/include/
	mv swift/$(PROJECT_NAME)FFI.modulemap swift/include/module.modulemap
	sed -i '' '1s/^/import LibXMTPRust\n\n/' swift/$(PROJECT_NAME).swift

.PHONY: $(ARCHS_IOS) $(ARCHS_MAC) framework all aarch64-apple-ios install-jar echo-jar download-toolchains swift
