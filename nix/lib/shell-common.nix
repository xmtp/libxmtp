# Shared building blocks for dev shells.
# Not a shell itself — exports attr sets that any shell can pick from.
{
  lib,
  stdenv,
  pkg-config,
  openssl,
  sqlite,
  zstd,
  zlib,
  llvmPackages,
  wasm-bindgen-cli,
  wasm-pack,
  binaryen,
  emscripten,
  wasm-tools,
  cargo-nextest,
  cargo-deny,
  cargo-machete,
  cargo-hakari,
  lcov,
  cargo-llvm-cov,
  buf,
  protobuf,
  protolint,
  taplo,
  shellcheck,
  nixfmt,
  lldb,
  vscode-extensions,
  gnuplot,
  flamegraph,
  cargo-flamegraph,
  inferno,
  jq,
  curl,
  graphite-cli,
  toxiproxy,
  omnix,
  rr,
  nixfmt-tree,
  markdownlint-cli,
}:
let
  inherit (stdenv) isLinux;
in
{
  # Core Rust build environment: env vars, hardening, native deps, LD_LIBRARY_PATH
  rustBase = {
    env = {
      OPENSSL_DIR = "${openssl.dev}";
      OPENSSL_LIB_DIR = "${lib.getLib openssl}/lib";
      OPENSSL_NO_VENDOR = 1;
      STACK_OVERFLOW_CHECK = 0;
      XMTP_NIX_ENV = "yes";
    };
    hardeningDisable = [
      "zerocallusedregs"
      "stackprotector"
    ];
    nativeBuildInputs = [
      pkg-config
      zstd
      openssl
      zlib
    ];
    buildInputs = [
      openssl
      sqlite
      zstd
    ];
    LD_LIBRARY_PATH = lib.makeLibraryPath [
      openssl
      zlib
    ];
  };

  # WASM cross-compilation env vars
  wasmEnv = {
    CC_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/clang";
    AR_wasm32_unknown_unknown = "${llvmPackages.bintools-unwrapped}/bin/llvm-ar";
    CFLAGS_wasm32_unknown_unknown = "-I ${llvmPackages.clang-unwrapped.lib}/lib/clang/21/include";
  };

  # WASM tooling
  wasmTools = [
    wasm-bindgen-cli
    wasm-pack
    binaryen
    emscripten
    wasm-tools
  ];

  # Cargo workflow tools
  cargoTools = [
    cargo-nextest
    cargo-deny
    cargo-machete
    cargo-hakari
  ];

  # CI-only cargo tools (coverage — Linux only)
  cargoCiTools = lib.optionals isLinux [
    lcov
    cargo-llvm-cov
  ];

  # Protobuf tools
  protoTools = [
    buf
    protobuf
    protolint
    markdownlint-cli
  ];

  # Lint tools
  lintTools = [
    taplo
    shellcheck
    nixfmt
  ];

  # Debugging & profiling tools
  debugTools = [
    lldb
    vscode-extensions.vadimcn.vscode-lldb
    gnuplot
    flamegraph
    cargo-flamegraph
    inferno
  ]
  ++ lib.optionals isLinux [ rr ];

  # Miscellaneous dev convenience tools
  miscDevTools = [
    jq
    curl
    graphite-cli
    toxiproxy
    omnix
  ];
}
