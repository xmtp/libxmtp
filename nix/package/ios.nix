# iOS cross-compilation package derivation.
# Builds static and dynamic libraries for 4 targets + Swift bindings, cacheable in Cachix.
# Assembles xcframeworks (static + dynamic) from built targets.
# Uses shared config from nix/lib/ios-env.nix and nix/lib/mobile-common.nix.
#
# Key derivations:
#   - Per-target static+dynamic libraries (impure — need Xcode SDK)
#   - Swift bindings (impure — runs uniffi-bindgen)
#   - Aggregate ios-libs (pure — symlinks to all targets)
#   - Static xcframework assembly (impure — needs Xcode for lipo + xcodebuild)
#   - Dynamic xcframework assembly (impure — needs Xcode for .framework wrapping)
#   - Release aggregate (pure — zip-ready directory layout)
#   - Dev aggregate (pure — bare xcframeworks for Package.swift detection)
#
{
  lib,
  xmtp,
  stdenv,
  ...
}:
let
  inherit (xmtp) iosEnv base;
  # override craneLib rust toolchain with given stdenv
  # https://crane.dev/API.html#mklib
  craneLib = xmtp.craneLib.overrideScope (
    _: _: {
      inherit stdenv;
    }
  );
  ffi-uniffi-bindgen = "${xmtp.ffi-uniffi-bindgen}/bin/ffi-uniffi-bindgen";
  # Rust toolchain with iOS/macOS cross-compilation targets.
  # No clippy/rustfmt — this is a build-only toolchain (the dev shell adds those).
  # overrideToolchain tells crane to use our custom fenix-based toolchain instead
  # of the default nixpkgs rustc.
  rust-toolchain = p: xmtp.mkToolchain p iosEnv.iosTargets [ ];
  rust = craneLib.overrideToolchain rust-toolchain;

  # Extract version once for use throughout the file
  version = xmtp.mkVersion rust;

  # Inherit shared config
  inherit (base) commonArgs bindingsFileset;

  # Build static (.a) and dynamic (.dylib) libraries for a single cross-compilation target.
  #
  # Uses crane's two-phase approach:
  #   1. buildDepsOnly — compiles all dependencies, outputs cargo artifacts.
  #      Cache key: Cargo.lock + Cargo.toml files + build.rs files.
  #      This is the expensive step (~30-60 min) that benefits from Cachix.
  #   2. buildPackage — compiles project source using cached dep artifacts.
  #      Uses the full source fileset and inherits cargoArtifacts from phase 1.
  #
  # IMPROVEMENT: buildDepsOnly could include an Xcode existence check in its
  # buildPhaseCargoCommand to provide a clearer error message if Xcode is missing,
  # rather than failing deep in the cc-rs build with cryptic SDK path errors.
  # Per-target dep cache — rebuilds only when Cargo.lock, Cargo.toml, or build.rs change.
  # Keyed by target triple. Impure (__noChroot) because build scripts need Xcode SDK.
  # Shared between buildTarget and swiftBindings (for aarch64-apple-darwin).
  mkDepsForTarget =
    target:
    let
      envSetup = iosEnv.envSetup target;
    in
    xmtp.base.mkCargoArtifacts rust false {
      pname = "xmtpv3-deps-${target}";
      CARGO_BUILD_TARGET = target;
      __noChroot = true;
      cargoExtraArgs = "--target ${target} -p xmtpv3";
      buildPhaseCargoCommand = ''
        ${envSetup}
        cargo build --locked --release --target ${target} -p xmtpv3
      '';
    };

  buildTarget =
    target:
    let
      cargoArtifacts = mkDepsForTarget target;
    in
    # Phase 2: Build project source using cached dep artifacts.
    rust.buildPackage (
      commonArgs
      // {
        inherit cargoArtifacts version;
        CARGO_BUILD_TARGET = target;
        __noChroot = true;
        pname = "xmtpv3-${target}";
        doInstallCargoArtifacts = false;
        src = bindingsFileset;
        cargoExtraArgs = "--target ${target} -p xmtpv3";
        # preBuild works here (unlike buildDepsOnly) because buildPackage doesn't
        # need to replace source files, so crane leaves the build hooks intact.
        preBuild = iosEnv.envSetup target;
        # Override crane's default installPhase which expects a binary executable.
        # We produce a static library (.a) and dynamic library (.dylib), so we copy them directly.
        installPhaseCommand = ''
          mkdir -p $out/${target}
          cp target/${target}/release/libxmtpv3.a $out/${target}/
          cp target/${target}/release/libxmtpv3.dylib $out/${target}/
          install_name_tool -id @rpath/libxmtpv3.dylib $out/${target}/libxmtpv3.dylib
        '';
      }
    );

  # Native host env setup for Swift bindings (builds for macOS host, not iOS).
  nativeEnvSetup = iosEnv.envSetup "aarch64-apple-darwin";

  # Swift bindings derivation.
  # Builds libxmtpv3 for the native macOS host, then runs uniffi-bindgen to
  # generate Swift bindings (.swift file + C header + modulemap).
  # Impure because it needs the Xcode SDK for the native host build.
  # The generated files are platform-independent — they work with any target's .a file.
  swiftBindings = rust.buildPackage (
    commonArgs
    // {
      pname = "xmtpv3-swift-bindings";
      __noChroot = true;
      src = bindingsFileset;
      inherit version;
      cargoArtifacts = mkDepsForTarget "aarch64-apple-darwin";
      cargoExtraArgs = "-p xmtpv3";
      CARGO_BUILD_TARGET = "aarch64-apple-darwin";
      buildPhaseCargoCommand = ''
        ${nativeEnvSetup}
        cargo build --release --target aarch64-apple-darwin -p xmtpv3
      '';
      # Prevent crane from trying to find and install cargo binaries from $out/bin.
      # This derivation produces generated source files, not executables.
      doNotPostBuildInstallCargoBinaries = true;
      postBuild = ''
        ${nativeEnvSetup}
        # Generate Swift bindings using uniffi-bindgen.
        # This runs the ffi-uniffi-bindgen binary (built above) against the compiled
        # static library to extract the FFI interface and produce:
        #   - xmtpv3.swift: Swift source with all public API types and functions
        #   - xmtpv3FFI.h: C header for the FFI layer
        #   - xmtpv3FFI.modulemap: Clang module map (renamed to module.modulemap)
        ${ffi-uniffi-bindgen} generate \
          --library target/aarch64-apple-darwin/release/libxmtpv3.a \
          --out-dir $TMPDIR/swift-out \
          --language swift
      '';
      installPhaseCommand = ''
        # Organize into expected directory structure for xcframework assembly
        mkdir -p $out/swift/include/libxmtp
        cp $TMPDIR/swift-out/xmtpv3.swift $out/swift/
        mv $TMPDIR/swift-out/xmtpv3FFI.h $out/swift/include/libxmtp/
        mv $TMPDIR/swift-out/xmtpv3FFI.modulemap $out/swift/include/libxmtp/module.modulemap
      '';
    }
  );

  # Function to build a specific set of targets (mirrors mkAndroid in android.nix)
  mkIos =
    targetList:
    let
      selectedTargets = lib.genAttrs targetList buildTarget;

      selectedAggregate = stdenv.mkDerivation {
        pname = "xmtpv3-ios-libs";
        inherit version;
        dontUnpack = true;
        doInstallCargoArtifacts = false;
        installPhase = ''
          mkdir -p $out/swift
          ${lib.concatMapStringsSep "\n" (target: ''
            mkdir -p $out/${target}
            ln -s ${selectedTargets.${target}}/${target}/libxmtpv3.a $out/${target}/libxmtpv3.a
            ln -s ${selectedTargets.${target}}/${target}/libxmtpv3.dylib $out/${target}/libxmtpv3.dylib
          '') targetList}
          ln -s ${swiftBindings}/swift/xmtpv3.swift $out/swift/xmtpv3.swift
          ln -s ${swiftBindings}/swift/include $out/swift/include
        '';
      };
    in
    {
      targets = selectedTargets;
      inherit swiftBindings;
      aggregate = selectedAggregate;
    };

  # Classify targets into platform groups for xcframework assembly.
  # Each non-empty group produces one platform slice in the xcframework.
  # Currently the only device target is aarch64-apple-ios; if more are added
  # they would need lipo like sim/macOS groups.
  classifyTargets =
    targetList:
    let
      device = builtins.filter (t: t == "aarch64-apple-ios") targetList;
      sim = builtins.filter (t: lib.hasSuffix "-ios-sim" t) targetList;
      mac = builtins.filter (t: lib.hasSuffix "-darwin" t) targetList;
    in
    {
      deviceTargets = device;
      simTargets = sim;
      macTargets = mac;
      expectedSlices =
        (if device != [ ] then 1 else 0) + (if sim != [ ] then 1 else 0) + (if mac != [ ] then 1 else 0);
    };

  # Shell preamble for xcframework derivations: resolves Xcode and adds
  # toolchain binaries (lipo, otool, install_name_tool) and /usr/bin (codesign) to PATH.
  xcframeworkEnvSetup = ''
    ${iosEnv.envSetup "aarch64-apple-darwin"}
    export PATH="$_XCODE_DEV/Toolchains/XcodeDefault.xctoolchain/usr/bin:/usr/bin:$PATH"
  '';

  # Assemble a static xcframework from per-target .a files.
  # Takes the target list, built target derivations, and swift bindings.
  # Produces $out/LibXMTPSwiftFFI.xcframework/
  mkStaticXcframework =
    targetList: selectedTargets: swiftBindings:
    let
      inherit (classifyTargets targetList)
        deviceTargets
        simTargets
        macTargets
        expectedSlices
        ;
      headerDir = "${swiftBindings}/swift/include/libxmtp";
    in
    stdenv.mkDerivation {
      pname = "xmtpv3-static-xcframework";
      inherit version;
      __noChroot = true;
      dontUnpack = true;
      dontFixup = true;
      installPhase = ''
        ${xcframeworkEnvSetup}
        set -euo pipefail

        echo "=== Building static xcframework ==="

        # lipo sim .a files into fat lib
        ${lib.optionalString (builtins.length simTargets > 0) ''
          mkdir -p $TMPDIR/lipo_sim
          lipo -create \
            ${lib.concatMapStringsSep " " (t: "${selectedTargets.${t}}/${t}/libxmtpv3.a") simTargets} \
            -output $TMPDIR/lipo_sim/libxmtpv3.a
        ''}

        # lipo macOS .a files into fat lib
        ${lib.optionalString (builtins.length macTargets > 0) ''
          mkdir -p $TMPDIR/lipo_macos
          lipo -create \
            ${lib.concatMapStringsSep " " (t: "${selectedTargets.${t}}/${t}/libxmtpv3.a") macTargets} \
            -output $TMPDIR/lipo_macos/libxmtpv3.a
        ''}

        # Build xcodebuild args
        XCF_ARGS=""
        ${lib.optionalString (deviceTargets != [ ]) ''
          XCF_ARGS="$XCF_ARGS -library ${
            selectedTargets.${"aarch64-apple-ios"}
          }/aarch64-apple-ios/libxmtpv3.a -headers ${headerDir}"
        ''}
        ${lib.optionalString (simTargets != [ ]) ''
          XCF_ARGS="$XCF_ARGS -library $TMPDIR/lipo_sim/libxmtpv3.a -headers ${headerDir}"
        ''}
        ${lib.optionalString (macTargets != [ ]) ''
          XCF_ARGS="$XCF_ARGS -library $TMPDIR/lipo_macos/libxmtpv3.a -headers ${headerDir}"
        ''}

        mkdir -p $out
        xcodebuild -create-xcframework \
          $XCF_ARGS \
          -output $out/LibXMTPSwiftFFI.xcframework

        # === Validation ===
        echo "Validating static xcframework..."
        FOUND=0
        for slice in $out/LibXMTPSwiftFFI.xcframework/*/; do
          [ -d "$slice/Headers" ] || continue
          FOUND=$((FOUND + 1))
          test -f "$slice/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing xmtpv3FFI.h in $slice"; exit 1; }
          test -f "$slice/Headers/module.modulemap" || { echo "FAIL: Missing modulemap in $slice"; exit 1; }
          head -1 "$slice/Headers/module.modulemap" | grep -q "module xmtpv3FFI" || \
            { echo "FAIL: Bad modulemap in $slice"; exit 1; }
          echo "  static OK: $(basename $slice)"
        done
        [ "$FOUND" -ge ${toString expectedSlices} ] || \
          { echo "FAIL: Expected >= ${toString expectedSlices} static slices, found $FOUND"; exit 1; }
        echo "Static xcframework validation passed ($FOUND slices)"

        chmod -R u+w $out
      '';
    };

  # Assemble a dynamic xcframework from per-target .dylib files.
  # Takes the target list, built target derivations, and swift bindings.
  # Produces $out/LibXMTPSwiftFFIDynamic.xcframework/
  mkDynamicXcframework =
    targetList: selectedTargets: swiftBindings:
    let
      inherit (classifyTargets targetList)
        deviceTargets
        simTargets
        macTargets
        expectedSlices
        ;
      headerDir = "${swiftBindings}/swift/include/libxmtp";

      # Shell snippet to wrap a dylib in a .framework bundle.
      # minOSVersion is required by App Store validation for embedded frameworks.
      mkFrameworkBundle = name: dylibPath: minOSVersion: ''
        echo "Building framework bundle: ${name}"
        mkdir -p $TMPDIR/${name}/xmtpv3FFI.framework/Headers
        mkdir -p $TMPDIR/${name}/xmtpv3FFI.framework/Modules
        cp ${dylibPath} $TMPDIR/${name}/xmtpv3FFI.framework/xmtpv3FFI
        install_name_tool -id @rpath/xmtpv3FFI.framework/xmtpv3FFI \
          $TMPDIR/${name}/xmtpv3FFI.framework/xmtpv3FFI
        cp ${headerDir}/*.h $TMPDIR/${name}/xmtpv3FFI.framework/Headers/
        sed 's/^module /framework module /' \
          ${headerDir}/module.modulemap \
          > $TMPDIR/${name}/xmtpv3FFI.framework/Modules/module.modulemap
        /usr/libexec/PlistBuddy \
          -c "Add :CFBundleExecutable string xmtpv3FFI" \
          -c "Add :CFBundleIdentifier string org.xmtp.xmtpv3FFI" \
          -c "Add :CFBundleInfoDictionaryVersion string 6.0" \
          -c "Add :CFBundleName string xmtpv3FFI" \
          -c "Add :CFBundlePackageType string FMWK" \
          -c "Add :CFBundleVersion string 1" \
          -c "Add :CFBundleShortVersionString string 1.0" \
          -c "Add :MinimumOSVersion string ${minOSVersion}" \
          $TMPDIR/${name}/xmtpv3FFI.framework/Info.plist
        codesign --force --sign - $TMPDIR/${name}/xmtpv3FFI.framework
      '';
    in
    stdenv.mkDerivation {
      pname = "xmtpv3-dynamic-xcframework";
      inherit version;
      __noChroot = true;
      dontUnpack = true;
      dontFixup = true;
      installPhase = ''
        ${xcframeworkEnvSetup}
        set -euo pipefail

        echo "=== Building dynamic xcframework ==="

        # lipo sim .dylib files into fat dylib
        ${lib.optionalString (builtins.length simTargets > 0) ''
          mkdir -p $TMPDIR/lipo_sim
          lipo -create \
            ${lib.concatMapStringsSep " " (t: "${selectedTargets.${t}}/${t}/libxmtpv3.dylib") simTargets} \
            -output $TMPDIR/lipo_sim/libxmtpv3.dylib
        ''}

        # lipo macOS .dylib files into fat dylib
        ${lib.optionalString (builtins.length macTargets > 0) ''
          mkdir -p $TMPDIR/lipo_macos
          lipo -create \
            ${lib.concatMapStringsSep " " (t: "${selectedTargets.${t}}/${t}/libxmtpv3.dylib") macTargets} \
            -output $TMPDIR/lipo_macos/libxmtpv3.dylib
        ''}

        # Build .framework bundles per platform
        ${lib.optionalString (deviceTargets != [ ]) (
          mkFrameworkBundle "fw_ios" "${
            selectedTargets.${"aarch64-apple-ios"}
          }/aarch64-apple-ios/libxmtpv3.dylib" "14.0"
        )}
        ${lib.optionalString (simTargets != [ ]) (
          mkFrameworkBundle "fw_sim" "$TMPDIR/lipo_sim/libxmtpv3.dylib" "14.0"
        )}
        ${lib.optionalString (macTargets != [ ]) (
          mkFrameworkBundle "fw_mac" "$TMPDIR/lipo_macos/libxmtpv3.dylib" "11.0"
        )}

        # Build xcodebuild args
        XCF_ARGS=""
        ${lib.optionalString (deviceTargets != [ ]) ''
          XCF_ARGS="$XCF_ARGS -framework $TMPDIR/fw_ios/xmtpv3FFI.framework"
        ''}
        ${lib.optionalString (simTargets != [ ]) ''
          XCF_ARGS="$XCF_ARGS -framework $TMPDIR/fw_sim/xmtpv3FFI.framework"
        ''}
        ${lib.optionalString (macTargets != [ ]) ''
          XCF_ARGS="$XCF_ARGS -framework $TMPDIR/fw_mac/xmtpv3FFI.framework"
        ''}

        mkdir -p $out
        xcodebuild -create-xcframework \
          $XCF_ARGS \
          -output $out/LibXMTPSwiftFFIDynamic.xcframework

        # === Validation ===
        echo "Validating dynamic xcframework..."
        FOUND=0
        for fw in $out/LibXMTPSwiftFFIDynamic.xcframework/*/xmtpv3FFI.framework; do
          test -d "$fw" || continue
          FOUND=$((FOUND + 1))
          test -f "$fw/xmtpv3FFI" || { echo "FAIL: Missing binary in $fw"; exit 1; }
          test -f "$fw/Info.plist" || { echo "FAIL: Missing Info.plist in $fw"; exit 1; }
          test -d "$fw/Headers" || { echo "FAIL: Missing Headers in $fw"; exit 1; }
          test -f "$fw/Headers/xmtpv3FFI.h" || { echo "FAIL: Missing xmtpv3FFI.h in $fw"; exit 1; }
          test -f "$fw/Modules/module.modulemap" || { echo "FAIL: Missing modulemap in $fw"; exit 1; }
          head -1 "$fw/Modules/module.modulemap" | grep -q "^framework module xmtpv3FFI" || \
            { echo "FAIL: modulemap missing 'framework module' prefix in $fw"; exit 1; }
          INSTALL_NAME=$(otool -D "$fw/xmtpv3FFI" | tail -1)
          echo "$INSTALL_NAME" | grep -q "@rpath/xmtpv3FFI.framework/xmtpv3FFI" || \
            { echo "FAIL: Bad install name '$INSTALL_NAME' in $fw"; exit 1; }
          echo "  dynamic OK: $(basename $(dirname $fw))"
        done
        [ "$FOUND" -ge ${toString expectedSlices} ] || \
          { echo "FAIL: Expected >= ${toString expectedSlices} dynamic slices, found $FOUND"; exit 1; }
        echo "Dynamic xcframework validation passed ($FOUND slices)"

        chmod -R u+w $out
      '';
    };

  # Aggregate for release: zip-ready directories with xcframeworks + Sources + LICENSE.
  mkRelease =
    {
      static,
      dynamic,
      swiftBindings,
      licenseFile,
    }:
    stdenv.mkDerivation {
      pname = "xmtpv3-ios-xcframeworks";
      inherit version;
      dontUnpack = true;
      installPhase = ''
        # Static zip contents
        mkdir -p $out/LibXMTPSwiftFFI/Sources/LibXMTP
        cp -r ${static}/LibXMTPSwiftFFI.xcframework $out/LibXMTPSwiftFFI/
        cp ${swiftBindings}/swift/xmtpv3.swift $out/LibXMTPSwiftFFI/Sources/LibXMTP/
        cp ${licenseFile} $out/LibXMTPSwiftFFI/LICENSE

        # Dynamic zip contents
        mkdir -p $out/LibXMTPSwiftFFIDynamic/Sources/LibXMTP
        cp -r ${dynamic}/LibXMTPSwiftFFIDynamic.xcframework $out/LibXMTPSwiftFFIDynamic/
        cp ${swiftBindings}/swift/xmtpv3.swift $out/LibXMTPSwiftFFIDynamic/Sources/LibXMTP/
        cp ${licenseFile} $out/LibXMTPSwiftFFIDynamic/LICENSE

        chmod -R u+w $out
      '';
    };

  # Aggregate for dev: bare xcframeworks at paths Package.swift expects.
  mkDev =
    {
      static,
      dynamic ? null,
      swiftBindings,
    }:
    stdenv.mkDerivation {
      pname = "xmtpv3-ios-xcframeworks-dev";
      inherit version;
      dontUnpack = true;
      installPhase = ''
        mkdir -p $out
        cp -r ${static}/LibXMTPSwiftFFI.xcframework $out/
        ${lib.optionalString (dynamic != null) ''
          cp -r ${dynamic}/LibXMTPSwiftFFIDynamic.xcframework $out/
        ''}
        # Include generated Swift bindings for dev script to copy to SDK source tree
        cp ${swiftBindings}/swift/xmtpv3.swift $out/xmtpv3.swift
        chmod -R u+w $out
      '';
    };

in
{
  inherit swiftBindings mkIos;
  # Default: all targets (for backward compat)
  inherit (mkIos iosEnv.iosTargets) targets aggregate;
  # Release: zip-ready directories for CI
  release =
    let
      ios = mkIos iosEnv.iosTargets;
    in
    mkRelease {
      static = mkStaticXcframework iosEnv.iosTargets ios.targets swiftBindings;
      dynamic = mkDynamicXcframework iosEnv.iosTargets ios.targets swiftBindings;
      inherit swiftBindings;
      licenseFile = ../../LICENSE;
    };
  # Dev fast: simulator + macOS only, static only
  devFast =
    let
      fastTargets = [
        "aarch64-apple-darwin"
        "aarch64-apple-ios-sim"
      ];
      ios = mkIos fastTargets;
    in
    mkDev {
      static = mkStaticXcframework fastTargets ios.targets swiftBindings;
      inherit swiftBindings;
    };
}
