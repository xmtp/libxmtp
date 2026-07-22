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

  # The xcframework slice list for a set of ABIs. `srcFor` maps a group key
  # ("device" | "sim" | "macos") to that slice's source artifact path.
  mkSlices =
    abis: srcFor:
    let
      inherit (classifyTargets abis) deviceAbis simAbis macAbis;
    in
    lib.optional (deviceAbis != [ ]) (
      mkSlice {
        platform = "ios";
        abis = deviceAbis;
      }
      // {
        group = "device";
        src = srcFor "device";
      }
    )
    ++ lib.optional (simAbis != [ ]) (
      mkSlice {
        platform = "ios";
        abis = simAbis;
        variant = "simulator";
      }
      // {
        group = "sim";
        src = srcFor "sim";
      }
    )
    ++ lib.optional (macAbis != [ ]) (
      mkSlice {
        platform = "macos";
        abis = macAbis;
      }
      // {
        group = "macos";
        src = srcFor "macos";
      }
    );

  # Device and simulator arm64 share an arch string; only LC_BUILD_VERSION
  # tells them apart. 1=macOS 2=iOS 7=iOS-simulator (numeric or symbolic
  # depending on the otool flavor).
  checkPlatformSnippet =
    slice: path:
    let
      want =
        if slice.platform == "macos" then
          "1|MACOS"
        else if slice.variant != null then
          "7|IOSSIMULATOR"
        else
          "2|IOS";
    in
    ''
      GOT_PLAT=$(otool -l ${path} | awk '/LC_BUILD_VERSION/{f=1} f && $1=="platform"{print $2; exit}')
      echo "$GOT_PLAT" | grep -qxE '${want}' || \
        { echo "FAIL: ${slice.id} has platform '$GOT_PLAT' (want ${want})"; exit 1; }
    '';

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
