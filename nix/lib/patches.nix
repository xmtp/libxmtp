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
  # Content-address the iOS SDK extracted from Xcode (from-scratch
  # apple-sdk path only; no-op for useiOSPrebuilt). The SDK output is
  # keyed on its contents, not on darwin.xcode's store hash — Apple
  # ships minor Xcode bundle revisions whose .xip hash drifts while the
  # embedded iPhoneOS.sdk stays byte-identical. Without CA every such
  # drift rebuilds the SDK and the entire iOS chain behind it. Requires
  # ca-derivations on the local daemon + Darwin builders. Upstream
  # nixpkgs keeps the derivation input-addressed by default, so this
  # stays an overlay.
  (
    final: prev:
    prev.lib.optionalAttrs prev.stdenv.hostPlatform.isiOS {
      apple-sdk = prev.apple-sdk.overrideAttrs (old: {
        src = old.src.overrideAttrs (_: {
          __contentAddressed = true;
          outputHashAlgo = "sha256";
          outputHashMode = "recursive";
        });
      });
    }
  )
]
