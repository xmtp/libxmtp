# The default shell for local development: the `rust` core plus Node tools,
# editor and debug tools, and the iOS environment on Darwin (the `ios` shell is
# an alias). No Android SDK: Android work uses the `android` shell.
{
  stdenv,
  lib,
  mkShell,
  mktemp,
  jdk21,
  diesel-cli,
  gnused,
  swiftformat,
  swiftlint,
  kotlin-language-server,
  xmtp,
  rust-analyzer,
  watchexec,
  oxfmt,
  rustShell,
}:
let
  inherit (stdenv) isDarwin;
  inherit (xmtp) shellCommon;
in
mkShell (
  shellCommon.rustShellEnv
  // {
    meta.description = "Full libXMTP local development environment";

    inputsFrom = [ rustShell ];

    XMTP_DEV_SHELL = "local";
    # The SDK bindgen formats generated TypeScript with this binary, the same
    # one the Nix render uses.
    XMTP_SDK_OXFMT = "${oxfmt}/bin/oxfmt";

    inherit (shellCommon.wasmEnv)
      WASM_BINDGEN_TEST_TIMEOUT
      RSTEST_TIMEOUT
      WASM_BINDGEN_TEST_WEBDRIVER_JSON
      CHROMEDRIVER
      ;

    buildInputs = [
      # JDK 21 for the Kotlin editor tools.
      jdk21
      (shellCommon.mkTools "default" (
        [
          rust-analyzer
          watchexec
          gnused
          kotlin-language-server
          swiftformat
          mktemp
          diesel-cli
        ]
        ++ shellCommon.debugTools
        ++ shellCommon.miscDevTools
        ++ lib.optionals isDarwin [ swiftlint ]
      ))
    ];

    # Warn about an Xcode older than 16 (Swift 6.1 Package Traits). The rust
    # shell's hook already set the Xcode compiler environment.
    shellHook = lib.optionalString isDarwin ''
      XCODE_VERSION=$(xcodebuild -version 2>/dev/null | head -1 | awk '{print $2}')
      if [[ -n "$XCODE_VERSION" && "''${XCODE_VERSION%%.*}" -lt 16 ]]; then
        echo "WARNING: Xcode $XCODE_VERSION detected. Xcode 16+ required for Swift 6.1 (Package Traits)." >&2
      fi
    '';
  }
)
