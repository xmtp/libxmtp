{
  inputs,
  self,
  lib,
  ...
}:
{
  flake.lib =
    let
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
      pkgConfig = {
        # Rust Overlay
        overlays = [
          inputs.fenix.overlays.default
          inputs.foundry.overlay
          self.overlays.default
          # mold is significantly faster on linux for local dev
          (
            final: prev:
            prev.lib.optionalAttrs prev.stdenv.isLinux {
              mkShell = prev.mkShell.override {
                stdenv = prev.stdenvAdapters.useMoldLinker prev.clangStdenv;
              };
            }
          )
          (final: prev: {
            #atf = prev.atf.overrideAttrs (old: {
            #  configureFlags = (old.configureFlags or [ ]) ++ [
            #    "kyua_cv_getopt_plus=yes"
            #  ];
            #});
            # # Override harfbuzz to disable tests - test-coretext fails with SIGABRT
            # # This fixes iOS/Android builds that transitively depend on harfbuzz
            # harfbuzz = prev.harfbuzz.overrideAttrs (old: {
            #   doCheck = false;
            #   mesonFlags = (old.mesonFlags or [ ]) ++ [ "-Dtests=disabled" ];
            # });
          })
          # tcl 8.6.16 (pinned via nixpkgs 09061f74...) has two cross-compile
          # bugs when targeting {x86_64,aarch64}-unknown-linux-musl, which
          # cascade into sqlite -> cargo-package-deps -> bindings-node-js-napi
          # / mls-validation-service -> devour-output:
          #
          #  1. compat/mkstemp.c calls strlen() without including <string.h>.
          #     gcc 15 promotes -Wimplicit-function-declaration to an error,
          #     breaking the musl cross-build (seen on x86_64-linux-musl from
          #     any host).
          #
          #  2. unix/tcl.m4's SC_CONFIG_SYSTEM macro reads `uname -s` on the
          #     build host to set tcl_cv_sys_version. When building on a
          #     Darwin runner (warp-macos-26-arm64-12x) this becomes Darwin-*,
          #     which selects the MAC_OSX_TCL / MAC_OSX_OBJS code path and
          #     causes macOS-only headers (mach/mach_time.h, libkern/*) to be
          #     compiled against a linux-musl sysroot.
          #
          # Remove this override when the nixpkgs pin is bumped past a rev
          # that adds the missing include and honors the autoconf host triple
          # in SC_CONFIG_SYSTEM.
          # See https://github.com/xmtp/libxmtp/issues/3444
          (
            final: prev:
            let
              patchedTcl86 = prev.tcl-8_6.overrideAttrs (old: {
                postPatch = (old.postPatch or "") + ''
                  substituteInPlace compat/mkstemp.c \
                    --replace-fail '#include <unistd.h>' '#include <unistd.h>
                  #include <string.h>'
                '';
                preConfigure =
                  (old.preConfigure or "")
                  + lib.optionalString prev.stdenv.hostPlatform.isLinux ''
                    # Override tcl's SC_CONFIG_SYSTEM autoconf cache: tcl reads
                    # `uname -s` from the *build* host (Darwin on CI), which
                    # would wrongly select the Darwin-* code path when
                    # cross-compiling to linux-musl. Force it to Linux so the
                    # configure script does not gate on MAC_OSX_TCL /
                    # TCL_WIDE_CLICKS and does not try to compile
                    # tclMacOSXFCmd.c or include mach/mach_time.h.
                    export tcl_cv_sys_version=Linux
                  '';
              });
            in
            {
              tcl-8_6 = patchedTcl86;
              tcl = patchedTcl86;
            }
          )
        ];
        config = {
          android_sdk.accept_license = true;
          allowUnfree = true;
        };
      };
      mkCrossPkgs =
        system: targets:
        let
          # Create pkgs for the build system to use applyPatches
          buildPkgs = import inputs.nixpkgs {
            inherit system;
            inherit (self.lib.pkgConfig) config;
          };
          # Apply Android NDK aarch64-darwin patch
          nixpkgs-patched = buildPkgs.applyPatches {
            name = "android-darwin-patch";
            src = inputs.nixpkgs;
            # can remove this patch once pull/505820 is merged into nixpkgs
            patches = [
              (buildPkgs.fetchpatch2 {
                url = "https://github.com/NixOS/nixpkgs/pull/505820.patch";
                sha256 = "sha256-1iEujs0metq+Q5dZc2yEzEdTdkQjntGaaBKW7WXwrAs=";
              })
            ];
          };
        in
        lib.listToAttrs (
          map (
            target:
            let
              t = normalize target;
            in
            {
              name = t.config;
              value = import nixpkgs-patched (
                self.lib.pkgConfig
                // {
                  localSystem = system;
                  crossSystem = t;
                }
              );
            }
          ) targets
        );
    };
  perSystem =
    {
      pkgs,
      ...
    }:
    let
      craneConfig = final: prev: {
        # add napi builder to crane scope
        napiBuild = final.callPackage ./napiBuild.nix { };
        uniffiGenerate = final.callPackage ./uniffiGenerate.nix { };
      };
      mkToolchain = pkgs.callPackage ./mkToolchain.nix { inherit inputs; };
    in
    {
      overlayAttrs = {
        xmtp = {
          inherit mkToolchain;
          # toolchain with native pkgs
          mkNativeToolchain = mkToolchain pkgs;
          filesets = pkgs.callPackage ./filesets.nix { };
          craneLib = (inputs.crane.mkLib pkgs).overrideScope craneConfig;
          base = pkgs.callPackage ./base.nix { };
          androidEnv = pkgs.callPackage ./android-env.nix { };
          iosEnv = pkgs.callPackage ./ios-env.nix { };
          ffi-uniffi-bindgen = pkgs.callPackage ./packages/uniffi-bindgen.nix { };
          shellCommon = pkgs.callPackage ./shell-common.nix { };
          mkVersion = import ./mkVersion.nix;
          toNapiTarget = import ./napiTarget.nix;
        };
        wasm-bindgen-cli = pkgs.callPackage ./packages/wasm-bindgen-cli.nix { };
        napi-rs-cli = pkgs.callPackage ./packages/napi-rs-cli { };
        ffi-uniffi-bindgen = pkgs.callPackage ./packages/uniffi-bindgen.nix { };
        swiftformat = pkgs.callPackage ./packages/swiftformat.nix { };
        swiftlint = pkgs.callPackage ./packages/swiftlint.nix { };
      };
    };
}
