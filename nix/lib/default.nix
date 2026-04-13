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
          # atf 0.23's configure.ac (m4/module-application.m4) uses AC_RUN_IFELSE
          # to check whether getopt(3) accepts a leading '+' for POSIX behavior
          # (cache var kyua_cv_getopt_plus). AC_RUN_IFELSE cannot execute the
          # compiled test binary during cross-compilation, aborting with:
          #   "configure: error: cannot run test program while cross compiling"
          #
          # This breaks the aarch64-apple-darwin cross-build chain:
          #   atf → libiconv → apple-sdk-14.4 → bindings-node-js-napi-*
          # See https://github.com/xmtp/libxmtp/issues/3470.
          #
          # All target platforms in this flake (Darwin, glibc Linux, musl Linux)
          # have a getopt that honours '+', so pre-seeding yes is safe.
          # Gated on cross-compilation so native builds keep pulling from
          # cache.nixos.org unchanged.
          (
            final: prev:
            prev.lib.optionalAttrs (prev.stdenv.buildPlatform != prev.stdenv.hostPlatform) {
              atf = prev.atf.overrideAttrs (old: {
                configureFlags = (old.configureFlags or [ ]) ++ [
                  "kyua_cv_getopt_plus=yes"
                ];
              });
            }
          )
          # tcl 8.6.16 (pinned via nixpkgs 09061f74...) has multiple
          # cross-compile bugs when targeting *-unknown-linux-musl, and the
          # Hydra build farm only caches the x86_64-linux build host (not
          # aarch64-darwin), so builds from a darwin host hit the bugs cold.
          # See https://github.com/xmtp/libxmtp/issues/3444.
          #
          # Symptoms seen in CI on warp-macos-26-arm64-12x:
          #   * compat/mkstemp.c: strlen() called without <string.h>; gcc 15
          #     promotes -Wimplicit-function-declaration to an error.
          #   * unix/configure's `uname -s` = "Darwin" on the build host
          #     defines TCL_WIDE_CLICKS + MAC_OSX_TCL even when cross-compiling
          #     to linux-musl, so tclUnixTime.c tries to include
          #     <mach/mach_time.h> against a linux sysroot.
          #
          # Rather than patching tcl itself — which requires fixing both the
          # generated configure script and the MAC_OSX_SRCS makefile variable,
          # and is fragile across nixpkgs revisions — we sidestep the build
          # entirely. sqlite only depends on tcl for its tclsqlite3 extension
          # and its test harness; libxmtp consumes libsqlite3 directly, so
          # --disable-tcl is safe. sqlite's autosetup uses the bundled jimsh0.c
          # for its own code generation when tcl is disabled.
          #
          # Override is gated on `hostPlatform.isMusl` so native sqlite on
          # linux/darwin keeps substituting from cache.nixos.org unchanged.
          (
            final: prev:
            prev.lib.optionalAttrs prev.stdenv.hostPlatform.isMusl {
              sqlite = prev.sqlite.overrideAttrs (old: {
                configureFlags =
                  (prev.lib.filter (f: !(prev.lib.hasPrefix "--with-tcl=" f)) old.configureFlags)
                  ++ [ "--disable-tcl" ];
                nativeBuildInputs = prev.lib.filter (p: !(prev.lib.hasPrefix "tcl" (p.pname or ""))) (
                  old.nativeBuildInputs or [ ]
                );
                doCheck = false;
              });
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
