{
  gnused,
  xmtp,
  stdenv,
  ...
}:
let
  inherit (xmtp) craneLib base;
  rust-toolchain = p: xmtp.mkToolchain p [ stdenv.hostPlatform.rust.rustcTarget ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;
  version = xmtp.mkVersion rust;

  inherit (base) bindingsFileset;
  commonArgs = base.commonArgs;

  specialArgs = {
    # set buildInputs to empty to force the android build to link against libraries in the NDK sysroot instead
    # of nix library path. This avoids compiling C dependencies by using android-native libs and having to patch nixpkgs packages.
    buildInputs = [ ];
  };

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false specialArgs;

  ext = if stdenv.isDarwin then "dylib" else "so";
  dylib = rust.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts version;
      pname = "xmtpv3-${stdenv.hostPlatform.rust.rustcTarget}";
      src = bindingsFileset;
      cargoExtraArgs = "-p xmtpv3";
      postFixup = ''
        ls $out/lib/
        cp $out/lib/libxmtpv3.${ext} $out/libuniffi_xmtpv3.${ext}
        rm -rf $out/lib
      '';
    }
    // specialArgs
  );

  kotlin-bindings = rust.uniffiGenerate {
    inherit version;
    pname = "xmtpv3-kotlin";
    language = "kotlin";
    dylibPath = "${dylib}/libuniffi_xmtpv3.${ext}";
    nativeBuildInputs = [ gnused ];
    postFixup = ''
      # Apply required sed replacements:
      # 1) Replace `return "xmtpv3"` with `return "uniffi_xmtpv3"` (library name fix)
      # 2) Replace `value.forEach { (k, v) ->` with `value.iterator().forEach { (k, v) ->`
      # 3) Suppress NewApi lint for java.lang.ref.Cleaner usage (requires API 33, minSdk is 23)
      # Note: uniffi outputs to uniffi/<crate_name>/<crate_name>.kt
      sed -i \
        -e 's/return "xmtpv3"/return "uniffi_xmtpv3"/' \
        -e 's/value\.forEach { (k, v) ->/value.iterator().forEach { (k, v) ->/g' \
        -e 's/@file:Suppress("NAME_SHADOWING")/@file:Suppress("NAME_SHADOWING", "NewApi")/' \
        "$out/kotlin/uniffi/xmtpv3/xmtpv3.kt"

      # Generate version file
      echo "Version: ${version}" > $out/libxmtp-version.txt
      echo "Date: $(date -u +%Y-%m-%d)" >> $out/libxmtp-version.txt
    '';
  };
in
{
  inherit kotlin-bindings dylib;
}
