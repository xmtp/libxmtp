{ stdenv
, darwin
, lib
, pkg-config
, mktemp
, jdk21
, kotlin
, diesel-cli
, gnuplot
, flamegraph
, cargo-flamegraph
, cargo-nextest
, cargo-deny
, inferno
, openssl
, sqlcipher
, sqlite
, corepack
, zstd
, foundry-bin
, graphite-cli
, jq
, llvmPackages
, curl
, buf
, protobuf
, protolint
, mkShell
, wasm-bindgen-cli
, wasm-pack
, binaryen
, emscripten
, taplo
, shellcheck
, lcov
, cargo-llvm-cov
, cargo-machete
, zlib
, xmtp
, omnix
, toxiproxy
, vscode-extensions
, lldb
, wasm-tools
, ...
}:
let
  inherit (stdenv) isDarwin isLinux;
  rust-toolchain = xmtp.mkToolchain [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ] [ "rust-src" "clippy-preview" "rust-docs" "rustfmt-preview" "llvm-tools-preview" ];
in
mkShell {
  OPENSSL_DIR = "${openssl.dev}";
  # disable -fzerocallusedregs in clang
  hardeningDisable = [ "zerocallusedregs" "stackprotector" ];
  OPENSSL_LIB_DIR = "${lib.getLib openssl}/lib";
  OPENSSL_NO_VENDOR = 1;
  STACK_OVERFLOW_CHECK = 0;
  CC_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/clang";
  AR_wasm32_unknown_unknown = "${llvmPackages.bintools-unwrapped}/bin/llvm-ar";
  CFLAGS_wasm32_unknown_unknown = "-I ${llvmPackages.clang-unwrapped.lib}/lib/clang/19/include";
  LD_LIBRARY_PATH = lib.makeLibraryPath [ openssl zlib ];
  nativeBuildInputs = [ pkg-config zstd openssl zlib ];
  XMTP_NIX_ENV = "yes";
  buildInputs =
    [
      rust-toolchain
      foundry-bin

      # native libs
      openssl
      sqlite
      sqlcipher
      zstd
      emscripten

      # Misc tools
      mktemp
      jdk21
      kotlin
      diesel-cli
      graphite-cli
      toxiproxy

      # Random devtools
      # tokio-console
      gnuplot
      flamegraph
      cargo-deny
      cargo-flamegraph
      cargo-nextest
      cargo-machete
      inferno
      jq
      curl
      lcov
      wasm-bindgen-cli
      binaryen
      wasm-pack
      binaryen
      vscode-extensions.vadimcn.vscode-lldb
      lldb

      # Protobuf
      buf
      protobuf
      protolint
      omnix

      # lint
      wasm-tools
      taplo
      # dev/up
      shellcheck
      # yarn/nodejs
      corepack
    ]
    ++ lib.optionals isDarwin [
      darwin.cctools
    ]
    ++ lib.optionals isLinux [ cargo-llvm-cov ];
}
