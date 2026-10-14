# The Rust core shell, for CI Rust jobs and as the base of `default` and
# `android` (through `inputsFrom` and `shellCommon.rustShellEnv`).
# Supports: dev/lint, dev/lint-rust, cargo test, cargo nextest, WASM checks,
# and the SDK checks, which also run Node and pnpm.
{
  mkShell,
  foundry-bin,
  xmtp-pnpm,
  nodejs_26,
  uv,
  xmtp,
}:
let
  inherit (xmtp) shellCommon;
in
mkShell (
  shellCommon.rustShellEnv
  // {
    meta.description = "Rust development environment for libXMTP crates and bindings";

    XMTP_DEV_SHELL = "rust";

    shellHook = shellCommon.rustShellHook;

    inherit (shellCommon.rustBase) nativeBuildInputs;

    buildInputs = shellCommon.rustCorePackages ++ [
      (shellCommon.mkTools "rust" (
        [
          foundry-bin
          xmtp-pnpm
          nodejs_26
          uv
        ]
        ++ shellCommon.wasmTools
        ++ shellCommon.agentTools
        ++ shellCommon.cargoCiTools
        ++ shellCommon.protoTools
        ++ shellCommon.lintTools
      ))
    ];
  }
)
