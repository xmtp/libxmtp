{
  inputs,
  self,
  lib,
  ...
}:
let
  patches = import ./patches.nix;

  craneConfig = final: prev: {
    # askama_derive 0.16 canonicalizes the askama.toml path through crane's
    # symlinked vendor dir, then emits include_bytes! with a lexical relative
    # path that resolves through the symlink target's real parents and lands
    # on a nonexistent path — breaking every #[derive(Template)] in
    # uniffi_bindgen. Dropping the canonicalize keeps the path lexical so the
    # relative include_bytes! stays inside the crate. Remove once askama
    # resolves symlinked-vendor-dir handling upstream.
    vendorCargoDeps =
      args:
      prev.vendorCargoDeps (
        args
        // {
          overrideVendorCargoPackage =
            p: drv:
            if p.name == "askama_derive" && lib.hasPrefix "0.16." p.version then
              drv.overrideAttrs (old: {
                postInstall = (old.postInstall or "") + ''
                  substituteInPlace $out/src/config.rs \
                    --replace-fail "filename.canonicalize().ok()" "Some(filename)"
                '';
              })
            else
              drv;
        }
      );
  };

  # `host` is the HOST pkgset (kept stable across cross pkgsets so build
  # tools stay host-built). `final` is the current pkgset — use it for
  # anything that must follow the target (toolchain, stdenv, crane).
  xmtpOverlay = host: final: _: rec {
    wasm-bindgen-cli = host.callPackage ./packages/wasm-bindgen-cli.nix { };
    napi-rs-cli = host.callPackage ./packages/napi-rs-cli { };
    swiftlint = host.callPackage ./packages/swiftlint.nix { };
    # Prebuilt binary — nixpkgs' swiftformat needs the Swift toolchain, which
    # is broken/uncached on Linux in current nixpkgs.
    swiftformat = host.callPackage ./packages/swiftformat.nix { };
    rtk = host.callPackage ./packages/rtk.nix { };
    kache = host.callPackage ./packages/kache.nix { };
    xmtp-pnpm = host.callPackage ./packages/pnpm.nix { };
    xmtp = {
      filesets = host.callPackage ./filesets.nix { };
      mkToolchain = final.callPackage ./mkToolchain.nix { inherit inputs; };
      mkNativeToolchain = xmtp.mkToolchain final;
      # The one toolchain of every Crane package. Crane calls it with each
      # spliced package set and compiles with `pkgsBuildHost`, whose
      # `hostPlatform` is the build machine, so the target comes from
      # `targetPlatform`. Native sets share one store path; a cross set adds
      # the `rust-std` of its own target. The default profile already has
      # clippy and rustfmt.
      hostToolchain =
        p:
        xmtp.mkToolchain p
          [ p.stdenv.targetPlatform.rust.rustcTarget "wasm32-unknown-unknown" ]
          [
            "llvm-tools"
          ];
      craneLib = (inputs.crane.mkLib final).overrideScope craneConfig;
      base = final.callPackage ./base.nix { };
      androidEnv = final.callPackage ./android-env.nix { };
      iosEnv = final.callPackage ./ios-env.nix { };
      shellCommon = host.callPackage ./shell-common.nix { };
      mkVersion = import ./mkVersion.nix;
      toNapiTarget = import ./napiTarget.nix;
      # Node platforms, Android ABIs, Apple slices, and the glibc floor.
      # Scripts and workflows read the same file. The glibc floor joins the
      # zigbuild target in a build command, so it must be MAJOR.MINOR digits,
      # which need no shell quoting.
      sdkPlatforms =
        let
          platforms = lib.importJSON ../sdk-platforms.json;
          floor = platforms.glibcFloor or null;
        in
        assert lib.assertMsg (
          builtins.isString floor && builtins.match "[0-9]+\\.[0-9]+" floor != null
        ) "nix/sdk-platforms.json: glibcFloor must be MAJOR.MINOR digits, not ${builtins.toJSON floor}";
        platforms;
      # The `@ubjs` runtimes of the pinned bindgen fork, built on the host.
      ubrn = host.callPackage ./packages/ubrn.nix { src = inputs.ubrn-src; };
      gitSha = self.shortRev or self.dirtyShortRev or "unknown";
      gitCommitDate = self.lastModifiedDate or "";
    };
  };

  baseOverlays = [
    inputs.fenix.overlays.default
    inputs.foundry.overlay
  ]
  ++ patches;

  config = {
    android_sdk.accept_license = true;
    allowUnfree = true;
  };

  mkHostPkgs =
    system:
    let
      hostPkgs = import inputs.nixpkgs {
        inherit system config;
        overlays = baseOverlays ++ [ (xmtpOverlay hostPkgs) ];
      };
    in
    hostPkgs;

  # crossSystem=null returns the host pkgset directly (no double import).
  mkXmtpPkgs =
    {
      system,
      crossSystem ? null,
    }:
    let
      hostPkgs = mkHostPkgs system;
    in
    if crossSystem == null then
      hostPkgs
    else
      import inputs.nixpkgs {
        inherit config crossSystem;
        localSystem = system;
        overlays = baseOverlays ++ [ (xmtpOverlay hostPkgs) ];
      };

  normalize =
    x:
    if builtins.isString x then
      { config = x; }
    else if builtins.isAttrs x then
      x
    else
      throw "expected a string or attribute set";
in
{
  flake.lib = {
    inherit mkXmtpPkgs;

    # The Xcode of the CI and release iOS outputs; sdks/ios/dev/bindings
    # compares it with the selected Xcode.
    inherit (import ./ios-env.nix { inherit lib; }) xcode;
    reactNativeXcode = (import ./react-native-env.nix { inherit lib; }).xcode;

    # Build cross pkgsets sharing one host pkgset across all targets.
    mkCrossPkgs =
      system: targets:
      let
        hostPkgs = mkHostPkgs system;
        overlays = baseOverlays ++ [ (xmtpOverlay hostPkgs) ];
      in
      lib.listToAttrs (
        map (
          target:
          let
            t = normalize target;
          in
          {
            name = t.config;
            value = import inputs.nixpkgs {
              inherit config overlays;
              localSystem = system;
              crossSystem = t;
            };
          }
        ) targets
      );
  };
}
