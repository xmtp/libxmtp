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
          # atf 0.23's configure.ac uses AC_RUN_IFELSE for three probes that
          # cannot execute a compiled test binary during cross-compilation,
          # aborting with:
          #   "configure: error: cannot run test program while cross compiling"
          #
          # This breaks the aarch64-apple-darwin cross-build chain:
          #   atf → libiconv → apple-sdk-14.4 → bindings-node-js-napi-*
          # See https://github.com/xmtp/libxmtp/issues/3470
          # and https://github.com/xmtp/libxmtp/issues/3476.
          #
          # The three AC_RUN_IFELSE cache variables and their justifications:
          #
          #   kyua_cv_getopt_plus (m4/module-application.m4)
          #     Tests whether getopt(3) accepts a leading '+' for POSIX
          #     behavior. All target platforms (Darwin, glibc, musl) honour '+'.
          #
          #   kyua_cv_attribute_noreturn (m4/module-defs.m4)
          #     Tests whether __attribute__((__noreturn__)) is supported by
          #     checking GCC version >= 2.5. All modern GCC/Clang satisfy this.
          #
          #   kyua_cv_getcwd_works (m4/module-fs.m4)
          #     Tests whether getcwd(NULL, 0) dynamically allocates. Both
          #     Darwin and Linux (glibc and musl) support this.
          #
          # Pre-seeding all three is safe for every target in this flake.
          # Gated on cross-compilation so native builds keep pulling from
          # cache.nixos.org unchanged.
          (
            final: prev:
            prev.lib.optionalAttrs (prev.stdenv.buildPlatform != prev.stdenv.hostPlatform) {
              atf = prev.atf.overrideAttrs (old: {
                configureFlags = (old.configureFlags or [ ]) ++ [
                  "kyua_cv_getopt_plus=yes"
                  "kyua_cv_attribute_noreturn=yes"
                  "kyua_cv_getcwd_works=yes"
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
        lib.listToAttrs (
          map (
            target:
            let
              t = normalize target;
            in
            {
              name = t.config;
              value = import inputs.nixpkgs (
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
        swiftlint = pkgs.callPackage ./packages/swiftlint.nix { };
      };
    };
}
