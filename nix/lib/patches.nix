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

  # harfbuzz's test-coretext requires access to macOS font services
  # (CTFontManagerRegisterFontsForURLs) which are unavailable in the
  # Nix sandbox, causing SIGABRT. Disable the test suite entirely on
  # Darwin via the meson 'tests' option.
  #
  # Pulled into the build graph via:
  #   remarshal → rich-argparse → rich → pytest-regressions → matplotlib
  #   → ffmpeg-headless → harfbuzz
  (
    final: prev:
    prev.lib.optionalAttrs prev.stdenv.hostPlatform.isDarwin {
      harfbuzz = prev.harfbuzz.overrideAttrs (old: {
        mesonFlags = (old.mesonFlags or [ ]) ++ [ "-Dtests=disabled" ];
      });
    }
  )
  # zvbi's configure auto-detects X11 and then compiles ntsc-cc.c which
  # #includes <X11/X.h>; in the Nix Darwin sandbox the probed
  # /usr/X11/include has no headers, so the compile fails. Force
  # --without-x — libxmtp doesn't need the X utilities.
  #
  # Pulled in via the same remarshal → matplotlib → ffmpeg-headless chain.
  (
    final: prev:
    prev.lib.optionalAttrs prev.stdenv.hostPlatform.isDarwin {
      zvbi = prev.zvbi.overrideAttrs (old: {
        configureFlags = (old.configureFlags or [ ]) ++ [ "--without-x" ];
      });
    }
  )
  # rsync's 'hardlinks' check-phase test is flaky in the Darwin Nix sandbox
  # (macOS filesystem semantics don't match rsync's expected linkage
  # behavior). Disable checks on Darwin — not needed by libxmtp.
  (
    final: prev:
    prev.lib.optionalAttrs prev.stdenv.hostPlatform.isDarwin {
      rsync = prev.rsync.overrideAttrs (_: {
        doCheck = false;
      });
    }
  )
  # LLVM 21.1.8's BPF BTF tests (func-func-ptr.ll, func-typedef.ll)
  # have stale expected output: a 2025-10-20 change to BTF generation
  # (llvm/llvm-project#155783, DW_TAG_variant_part support) changed
  # emitted offsets, but the test .ll files weren't regenerated.
  # FileCheck fails on hardcoded `.long` sequences. Not a miscompile —
  # only test-metadata drift, and the BPF backend is unreachable from
  # iOS cross-compilation anyway. Delete the two stale tests via
  # postPatch so `check-all` passes.
  (
    final: prev:
    prev.lib.optionalAttrs prev.stdenv.hostPlatform.isDarwin {
      llvmPackages_21 = prev.llvmPackages_21 // {
        libllvm = prev.llvmPackages_21.libllvm.overrideAttrs (old: {
          postPatch = (old.postPatch or "") + ''
            rm test/CodeGen/BPF/BTF/func-func-ptr.ll
            rm test/CodeGen/BPF/BTF/func-typedef.ll
          '';
        });
      };
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
