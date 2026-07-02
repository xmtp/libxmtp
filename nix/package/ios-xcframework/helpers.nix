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

  # Map an ABI key to its Mach-O architecture name.
  abiArch = abi: if lib.hasPrefix "x86_64" abi then "x86_64" else "arm64";

  # Slice metadata mirroring xcodebuild's LibraryIdentifier convention:
  # <platform>-<arch>[_<arch>…][-<variant>], archs sorted alphabetically.
  mkSlice =
    {
      platform,
      abis,
      variant ? null,
    }:
    let
      archs = lib.naturalSort (lib.unique (map abiArch abis));
    in
    {
      inherit platform variant archs;
      id = lib.concatStringsSep "-" (
        [
          platform
          (lib.concatStringsSep "_" archs)
        ]
        ++ lib.optional (variant != null) variant
      );
    };

  # Top-level xcframework manifest — the only thing
  # `xcodebuild -create-xcframework` adds beyond copying slices into place.
  mkXCFrameworkPlist =
    slices:
    writeText "xcframework-Info.plist" (
      lib.generators.toPlist { escape = true; } {
        AvailableLibraries = map (
          s:
          {
            BinaryPath = s.binaryPath;
            LibraryIdentifier = s.id;
            LibraryPath = s.libraryPath;
            SupportedArchitectures = s.archs;
            SupportedPlatform = s.platform;
          }
          // lib.optionalAttrs (s ? headersPath) { HeadersPath = s.headersPath; }
          // lib.optionalAttrs (s.variant != null) { SupportedPlatformVariant = s.variant; }
        ) slices;
        CFBundlePackageType = "XFWK";
        XCFrameworkFormatVersion = "1.0";
      }
    );

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
    minOSVersion: isMacOS:
    writeText "Info.plist" (
      lib.generators.toPlist { escape = true; } (
        {
          CFBundleExecutable = "xmtpv3FFI";
          CFBundleIdentifier = "org.xmtp.xmtpv3FFI";
          CFBundleInfoDictionaryVersion = "6.0";
          CFBundleName = "xmtpv3FFI";
          CFBundlePackageType = "FMWK";
          CFBundleVersion = "1";
          CFBundleShortVersionString = "1.0";
        }
        # macOS frameworks declare LSMinimumSystemVersion; iOS uses
        # MinimumOSVersion.
        // (
          if isMacOS then { LSMinimumSystemVersion = minOSVersion; } else { MinimumOSVersion = minOSVersion; }
        )
      )
    );

  # Wrap a (possibly lipo'd) dylib in a .framework bundle, one per
  # xcframework slice. iOS frameworks are flat; macOS frameworks require
  # the versioned Versions/A layout with top-level symlinks and
  # Info.plist under Resources/.
  mkFrameworkBundle =
    {
      name,
      dylibPath,
      minOSVersion,
      headerDir,
      isMacOS ? false,
    }:
    let
      fw = "$TMPDIR/${name}/xmtpv3FFI.framework";
      contentDir = if isMacOS then "${fw}/Versions/A" else fw;
      plistDest = if isMacOS then "${contentDir}/Resources/Info.plist" else "${fw}/Info.plist";
      dylibId =
        if isMacOS then
          "@rpath/xmtpv3FFI.framework/Versions/A/xmtpv3FFI"
        else
          "@rpath/xmtpv3FFI.framework/xmtpv3FFI";
    in
    ''
      echo "Building framework bundle: ${name}"
      mkdir -p ${contentDir}/Headers ${contentDir}/Modules
      ${lib.optionalString isMacOS "mkdir -p ${contentDir}/Resources"}
      cp ${dylibPath} ${contentDir}/xmtpv3FFI
      # store copies are read-only; install_name_tool and rcodesign both
      # rewrite the binary in place
      chmod u+w ${contentDir}/xmtpv3FFI
      install_name_tool -id ${dylibId} ${contentDir}/xmtpv3FFI
      cp ${headerDir}/*.h ${contentDir}/Headers/
      sed 's/^module /framework module /' \
        ${headerDir}/module.modulemap \
        > ${contentDir}/Modules/module.modulemap
      cp ${mkInfoPlist minOSVersion isMacOS} ${plistDest}
      ${lib.optionalString isMacOS ''
        ln -s A ${fw}/Versions/Current
        ln -s Versions/Current/xmtpv3FFI ${fw}/xmtpv3FFI
        ln -s Versions/Current/Headers ${fw}/Headers
        ln -s Versions/Current/Modules ${fw}/Modules
        ln -s Versions/Current/Resources ${fw}/Resources
      ''}
      rcodesign sign ${fw}
    '';
}
