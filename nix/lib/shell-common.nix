# Shared building blocks for dev shells.
# Not a shell itself — exports attr sets that any shell can pick from.
{
  lib,
  stdenv,
  buildEnv,
  sqlcipher,
  darwin,
  git,
  just,
  python311,
  openssl,
  zlib,
  llvmPackages,
  wasm-bindgen-cli,
  cargo-nextest,
  cargo-deny,
  cargo-machete,
  cargo-hakari,
  kache,
  sqlx-cli,
  grpc-health-probe,
  grpcurl,
  lcov,
  cargo-llvm-cov,
  buf,
  protobuf,
  protolint,
  taplo,
  shellcheck,
  ruff,
  nixfmt,
  lldb,
  vscode-extensions,
  flamegraph,
  cargo-flamegraph,
  inferno,
  gh,
  jq,
  rtk,
  curl,
  git-cliff,
  toxiproxy,
  rr,
  markdownlint-cli,
  chromedriver,
  google-chrome,
  chromium,
  writeText,
  xmtp,
}:
let
  inherit (stdenv) isLinux;

  # Pin Chrome binary to the Nix-provided version so ChromeDriver never
  # discovers a system-installed Chrome with a different major version.
  chromeBin =
    if stdenv.isDarwin then
      "${lib.getBin google-chrome}/bin/google-chrome-stable"
    else
      "${lib.getBin chromium}/bin/chromium";

  # Read the base webdriver.json and inject the Nix Chrome binary path.
  baseWebdriver = builtins.fromJSON (builtins.readFile ./../../webdriver.json);
  webdriverJson = writeText "webdriver.json" (
    builtins.toJSON (
      baseWebdriver
      // {
        "goog:chromeOptions" = baseWebdriver."goog:chromeOptions" // {
          binary = chromeBin;
        };
      }
    )
  );
  # Core Rust build environment: env vars, hardening, native deps, LD_LIBRARY_PATH
  rustBase = xmtp.base.commonArgs // {
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
    WASM_BINDGEN_TEST_TIMEOUT = 1024;
    RSTEST_TIMEOUT = 90;
    WASM_BINDGEN_TEST_WEBDRIVER_JSON = webdriverJson;
    CHROMEDRIVER = "${lib.getBin chromedriver}/bin/chromedriver";
  };

in
rec {
  inherit rustBase wasmEnv;

  # WASM tooling: `wasm-bindgen` and the cargo test runner's browser.
  wasmTools = [
    wasm-bindgen-cli
    chromedriver
  ]
  # chromium unsupported on darwin
  # google-chrome unsupported on aarch64-linux
  # Firefox compiles from scratch on everything but x86_64 (unreliable build)
  ++ lib.optionals stdenv.isDarwin [ google-chrome ]
  ++ lib.optionals stdenv.isLinux [ chromium ];

  # Agent command filters. Keep these out of build derivations.
  agentTools = [ rtk ];

  # Cargo workflow tools
  cargoTools = [
    cargo-nextest
    cargo-deny
    cargo-machete
    cargo-hakari
    sqlx-cli
    grpc-health-probe
    grpcurl
    kache
  ];

  # The one Rust toolchain of the rust, default, and android shells: the
  # host, wasm32, the Android ABIs, and on Darwin the Apple slices.
  rustToolchain =
    xmtp.mkNativeToolchain
      (
        [
          "wasm32-unknown-unknown"
          "x86_64-unknown-linux-gnu"
        ]
        ++ xmtp.androidEnv.androidTargets
        ++ lib.optionals stdenv.isDarwin xmtp.iosEnv.iosTargets
      )
      [
        "rust-src"
        "clippy-preview"
        "rustfmt-preview"
        "llvm-tools-preview"
      ];

  # The packages of every Rust shell: the toolchain, the native build inputs,
  # the cargo tools, git, Python, and just. `rust` adds the CI and SDK tools;
  # `android` adds the Android SDK.
  rustCorePackages =
    rustBase.buildInputs
    ++ [
      rustToolchain
      # In-shell Nix fetchGit must use Nix git with this shell's LD_LIBRARY_PATH.
      git
      just
      (python311.withPackages (packages: [ packages.pyyaml ]))
    ]
    ++ cargoTools
    ++ [ sqlcipher ]
    ++ lib.optionals stdenv.isDarwin [ darwin.cctools ];

  # A shell's other tools, as one package that links only their `bin`. Their
  # libraries then add nothing to PKG_CONFIG_PATH (or to the compiler search
  # paths), which C build scripts track, so the Rust shells hold the same
  # values and a switch between them rebuilds no C code.
  mkTools =
    name: paths:
    buildEnv {
      name = "${name}-tools";
      inherit paths;
      pathsToLink = [ "/bin" ];
    };

  # The environment that every Rust shell sets, so a switch between them
  # changes no variable that a C build script tracks. `inputsFrom` passes
  # only packages and shellHook, so each shell merges this set.
  rustShellEnv = {
    inherit (rustBase) hardeningDisable LD_LIBRARY_PATH;
    # Kache, the compiler cache. Only the shells set it; Nix build
    # derivations keep their own caches. Preferences live in kache.toml, the
    # lowest Kache file layer. The settings below keep builds correct, so
    # they are variables, which win over every Kache file. In CI, setup-nix
    # sets KACHE_DISABLED=1 for jobs without Kache and turns incremental off.
    RUSTC_WRAPPER = "${kache}/bin/kache";
    KACHE_HOST_CONFIG = "${./kache.toml}";
    # Build-script runs have no config-file key.
    KACHE_BUILD_SCRIPT_CACHE = 0;
    # Proc macros read these without emitting rustc env-dep records.
    KACHE_KEY_ENV_VARS = "CI,XMTP_TEST_LOGGING";
    inherit (wasmEnv)
      CC_wasm32_unknown_unknown
      AR_wasm32_unknown_unknown
      CFLAGS_wasm32_unknown_unknown
      ;
  }
  // rustBase.env;

  # The shellHook of every Rust shell: on Darwin one compiler environment,
  # Xcode clang with the 11.0 macOS floor, for the host and the Apple slices.
  # SDKROOT stays unset so xcrun picks the SDK per target.
  rustShellHook = lib.optionalString stdenv.isDarwin ''
    unset SDKROOT
    ${xmtp.iosEnv.envSetupAll}
  '';

  # Coverage tools for `just coverage` and the CI coverage run.
  cargoCiTools = [
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
    # dev/tests/test_script_lint.py lints extensionless Python scripts.
    ruff
    nixfmt
  ];

  # Debugging & profiling tools
  debugTools = [
    lldb
    vscode-extensions.vadimcn.vscode-lldb
    flamegraph
    cargo-flamegraph
    inferno
  ]
  ++ lib.optionals isLinux [ rr ];

  # Miscellaneous dev convenience tools
  miscDevTools = [
    gh
    jq
    curl
    git-cliff # local changelog generation (cliff.toml)
    toxiproxy
  ];
}
