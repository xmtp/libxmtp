PROJECT_NAME = xmtpv3

# Simulator config
ARCHS_IOS = x86_64-apple-ios aarch64-apple-ios-sim
ARCHS_MAC = x86_64-apple-darwin aarch64-apple-darwin
LIB=libxmtpv3.a
JAR_DIR=$(shell pwd)/tests/jar
SQLCIPHER_DIR=$(shell pwd)/sqlcipher
SQLCIPHER_LIB=$(SQLCIPHER_DIR)/.libs/libsqlcipher.a
GIT_BRANCH=$(shell git rev-parse --abbrev-ref HEAD)
GIT_COMMIT_HASH=$(shell git log -1 --pretty=format:"%h")
GIT_COMMIT_DATE=$(shell TZ=UTC git log -1 --date=iso-local --pretty=format:"%ad")

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

download-sqlcipher:
	git clone https://github.com/sqlcipher/sqlcipher.git $(SQLCIPHER_DIR)

build-sqlcipher:
	cd $(SQLCIPHER_DIR) && \
	./configure --enable-tempstore=yes CFLAGS="-DSQLITE_HAS_CODEC" && \
	make

all: framework

libxmtp-version:
	echo "Version: $(GIT_COMMIT_HASH)\nBranch: $(GIT_BRANCH)\nDate: $(GIT_COMMIT_DATE)" > libxmtp-version.txt

$(ARCHS_IOS): %: build-sqlcipher
	cross build --target $@ --target-dir ./target --release --no-default-features
	mkdir -p build/$@
	mv target/$@/release/$(LIB) build/$@/$(LIB)

$(ARCHS_MAC): %: build-sqlcipher
	cross build --target $@ --target-dir ./target --release --no-default-features
	mkdir -p build/$@
	mv target/$@/release/$(LIB) build/$@/$(LIB)

aarch64-apple-ios: build-sqlcipher
	cross build --target $@ --target-dir ./target --release
	mkdir -p build/$@
	mv target/$@/release/$(LIB) build/$@/$(LIB)

$(LIB): $(ARCHS_IOS) $(ARCHS_MAC) aarch64-apple-ios

# lipo combines libs for different architectures (aarch64, x86_64, ...) into one fat lib
lipo:
	mkdir -p build/lipo_macos build/lipo_ios_sim
	lipo -create -output build/lipo_ios_sim/$(LIB) $(foreach arch,$(ARCHS_IOS),$(wildcard build/$(arch)/$(LIB)))
	lipo -create -output build/lipo_macos/$(LIB) $(foreach arch,$(ARCHS_MAC),$(wildcard build/$(arch)/$(LIB)))

# xcframework combines libs for different platforms (iOS, iOS-simulator, macOS, ...) into one framework that can be used in Xcode
framework: lipo
	rm -rf LibXMTPSwiftFFI.xcframework
	xcodebuild -create-xcframework \
		-library build/aarch64-apple-ios/$(LIB) \
		-headers build/swift/include/ \
		-library build/lipo_ios_sim/$(LIB) \
		-headers build/swift/include/ \
		-library build/lipo_macos/$(LIB) \
		-headers build/swift/include/ \
		-output LibXMTPSwiftFFI.xcframework

# build uniffi bindings for swift
swift: libxmtp-version
	cargo build --release
	rm -rf build/swift
	target/release/ffi-uniffi-bindgen generate \
		--lib-file target/release/$(LIB) \
		src/$(PROJECT_NAME).udl \
		--out-dir build/swift \
		--language swift
	# https://mozilla.github.io/uniffi-rs/swift/module.html#compiling-a-swift-module
	mkdir -p build/swift/include
	mv build/swift/$(PROJECT_NAME)FFI.h build/swift/include/
	mv build/swift/$(PROJECT_NAME)FFI.modulemap build/swift/include/module.modulemap
	cp libxmtp-version.txt build/swift/

swiftlocal: libxmtpv3.a swift framework 

.PHONY: $(ARCHS_IOS) $(ARCHS_MAC) framework all aarch64-apple-ios install-jar echo-jar download-toolchains swift lipo download-sqlcipher build-sqlcipher
