{ lib, writeText }:
rec {
  # Classify ABI keys into platform groups for xcframework slicing.
  # Each non-empty group becomes one slice in the produced xcframework.
  classifyTargets =
    abis:
    let
      deviceAbis = builtins.filter (a: a == "iphone64") abis;
      simAbis = builtins.filter (a: a == "iphone64-simulator") abis;
      macAbis = builtins.filter (a: lib.hasSuffix "-darwin" a) abis;
    in
    {
      inherit deviceAbis simAbis macAbis;
      expectedSlices = lib.count (g: g != [ ]) [
        deviceAbis
        simAbis
        macAbis
      ];
    };

  # Lipo a list of per-ABI artifacts (.a or .dylib) into a single fat binary.
  # Emits empty shell when group is empty.
  mkLipoSnippet =
    {
      group,
      ext,
      dylibs,
      abis,
    }:
    lib.optionalString (abis != [ ]) ''
      mkdir -p $TMPDIR/lipo_${group}
      lipo -create \
        ${lib.concatMapStringsSep " " (abi: "${dylibs.${abi}}/libxmtpv3.${ext}") abis} \
        -output $TMPDIR/lipo_${group}/libxmtpv3.${ext}
    '';

  # Build the framework's Info.plist as a structural Nix attrset, rendered via
  # lib.generators.toPlist. Keeps escaping and formatting consistent with nixpkgs
  # plist conventions instead of an inline heredoc.
  mkInfoPlist =
    minOSVersion:
    writeText "Info.plist" (
      lib.generators.toPlist { escape = true; } {
        CFBundleExecutable = "xmtpv3FFI";
        CFBundleIdentifier = "org.xmtp.xmtpv3FFI";
        CFBundleInfoDictionaryVersion = "6.0";
        CFBundleName = "xmtpv3FFI";
        CFBundlePackageType = "FMWK";
        CFBundleVersion = "1";
        CFBundleShortVersionString = "1.0";
        MinimumOSVersion = minOSVersion;
      }
    );

  # Wrap a (possibly lipo'd) dylib in a .framework bundle for
  # xcodebuild -create-xcframework -framework <fw>.
  mkFrameworkBundle =
    {
      name,
      dylibPath,
      minOSVersion,
      headerDir,
    }:
    ''
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
      cp ${mkInfoPlist minOSVersion} $TMPDIR/${name}/xmtpv3FFI.framework/Info.plist
      rcodesign sign $TMPDIR/${name}/xmtpv3FFI.framework
    '';
}
