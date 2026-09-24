{
  lib,
  buildNpmPackage,
  rustPlatform,
  stdenv,
}:
let
  # Keep the bindgen Cargo dependency and all three runtimes at this revision.
  rev = "330f9edbc3c4d6e6e948f6d2eb2724358668b79a";
  src = builtins.fetchGit {
    url = "https://github.com/neekolas/uniffi-bindgen-react-native.git";
    inherit rev;
  };
  core = buildNpmPackage {
    pname = "ubjs-core";
    version = rev;
    inherit src;
    sourceRoot = "source/typescript";
    npmDepsHash = "sha256-SI1EVkaeiGPd9WOU+EUzSbWda015v5Kt4rM8hM5gUL8=";
    npmFlags = [ "--ignore-scripts" ];
  };
  wasm = buildNpmPackage {
    pname = "ubjs-wasm";
    version = rev;
    inherit src;
    sourceRoot = "source/runtimes/wasm";
    npmDepsHash = "sha256-p3gL/LvN/6N//2YIOoeKIuNvacgtfYKzF990gb92cpk=";
    npmFlags = [ "--ignore-scripts" ];
    preBuild = ''
      rm -r node_modules/@ubjs/core
      ln -s ${core}/lib/node_modules/@ubjs/core node_modules/@ubjs/core
    '';
  };
  nodeNative = rustPlatform.buildRustPackage {
    pname = "ubjs-node-native";
    version = rev;
    inherit src;
    cargoLock.lockFile = src + /Cargo.lock;
    buildFeatures = [ ];
    cargoBuildFlags = [
      "-p"
      "uniffi-runtime-napi"
    ];
    doCheck = false;
    installPhase = ''
      mkdir -p $out
      binary=$(find target -type f \( -name 'libuniffi_runtime_napi.dylib' -o -name 'libuniffi_runtime_napi.so' \) -print -quit)
      test -n "$binary"
      cp "$binary" $out/
    '';
  };
  node = buildNpmPackage {
    pname = "ubjs-node";
    version = rev;
    inherit src;
    sourceRoot = "source/runtimes/napi";
    npmDepsHash = "sha256-NUDHnGmtvmeDjlWr3ubz31Sd9YS6ZGqVRRGU3CzUqy8=";
    npmFlags = [ "--ignore-scripts" ];
    npmBuildScript = "build:ts";
    postInstall = ''
      package=$out/lib/node_modules/@ubjs/node
      mkdir -p "$package"
      cp ${nodeNative}/libuniffi_runtime_napi.* "$package/uniffi-runtime-napi.${
        if stdenv.hostPlatform.isDarwin then "darwin-arm64" else "linux-x64-gnu"
      }.node"
    '';
  };
in
{
  inherit
    rev
    src
    core
    node
    wasm
    ;
}
