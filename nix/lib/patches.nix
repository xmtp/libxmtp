[
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
  # Upstreamed as https://github.com/NixOS/nixpkgs/pull/510292 (merged,
  # present in the current pin) — but the upstream seeds are gated on
  # `!buildPlatform.canExecute hostPlatform`, and our nightly cross is
  # aarch64-darwin → aarch64-apple-darwin: same arch and kernel, so
  # canExecute is TRUE and the seeds never apply, while autoconf still
  # decides cross_compiling=yes from the triple mismatch and aborts
  # (broke the 2026-07-03 nightly after #3810 dropped this overlay).
  # Keep this structurally-gated copy until the upstream gate is
  # `buildPlatform != hostPlatform`; double-applying the seeds when
  # both gates fire is harmless (same cache values).
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
  # nixpkgs' cc-wrapper bakes -mtls-dialect=gnu2 into the wrapper as a
  # machine flag for every x86 Linux target when it believes the compiler
  # is clang >= 19.1 (pkgs/build-support/cc-wrapper/default.nix,
  # `tlsDialect`). Both halves of that check misfire for the Android NDK
  # toolchain:
  #   * androidndk-pkgs' wrapped cc inherits the *NDK* version (27.x) as
  #     its compiler version, so the ">= 19.1" gate passes even though
  #     NDK r27 ships clang 18;
  #   * clang rejects the flag for *-linux-android triples at any version
  #     ("unsupported argument 'gnu2' to option '-mtls-dialect='") —
  #     TLSDESC-via-gnu2 is a glibc/musl mechanism, not bionic.
  # Every C compile for the x86 Android targets then fails; first casualty
  # is ring's build script inside cargo-package-deps-x86_64-unknown-linux-android.
  # Strip the flag from the wrapper's substituted flag script. Gated on
  # x86 Android host platforms so every other pkgset keeps its cache.
  # Drop when nixpkgs excludes Android from `tlsDialect` (or androidndk
  # reports the real clang version).
  (
    final: prev:
    prev.lib.optionalAttrs (prev.stdenv.hostPlatform.isAndroid && prev.stdenv.hostPlatform.isx86) {
      stdenv = prev.overrideCC prev.stdenv (
        prev.stdenv.cc.overrideAttrs (old: {
          postFixup = (old.postFixup or "") + ''
            for f in $out/nix-support/add-local-cc-cflags-before.sh $out/nix-support/cc-cflags-before; do
              if [ -f "$f" ]; then
                sed -i "s| *'-mtls-dialect=gnu2'||g; s| *-mtls-dialect=gnu2||g" "$f"
              fi
            done
          '';
        })
      );
    }
  )

]
