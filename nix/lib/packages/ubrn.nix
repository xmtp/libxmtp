{
  lib,
  buildNpmPackage,
  rustPlatform,
  napi-rs-cli,
  nodejs,
}:
let
  # Cargo.lock records the exact revision of the generator dependency.
  lock = builtins.fromTOML (builtins.readFile ../../../Cargo.lock);
  bindgen = lib.findFirst (
    package: package.name == "ubrn_bindgen"
  ) (throw "Cargo.lock has no ubrn_bindgen package") lock.package;
  lockedRev = builtins.elemAt (lib.splitString "#" bindgen.source) 1;
  expectedRev =
    (builtins.fromTOML (builtins.readFile ../../../Cargo.toml)).workspace.metadata."xmtp-sdk-fork".rev;
  rev =
    if lockedRev == expectedRev then lockedRev else throw "SDK fork lock differs from workspace pin";
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
    nativeBuildInputs = [
      napi-rs-cli
      nodejs
    ];
    doCheck = false;
    postBuild = ''
      # nixpkgs provides napi CLI 3. Its default targets already include the
      # targets repeated in this fork's napi CLI 2 configuration.
      node -e 'const fs = require("fs"); const path = "runtimes/napi/package.json"; const config = JSON.parse(fs.readFileSync(path)); config.napi.triples.additional = []; fs.writeFileSync(path, JSON.stringify(config));'
      cd runtimes/napi
      napi build --platform --release --js-package-name @ubjs/node
      cd ../..
    '';
    installPhase = ''
      mkdir -p $out
      binary=$(find runtimes/napi -maxdepth 1 -name '*.node' -print -quit)
      test -n "$binary"
      cp "$binary" $out/
    '';
  };
  cli = rustPlatform.buildRustPackage {
    pname = "ubrn";
    version = rev;
    inherit src;
    cargoLock.lockFile = src + /Cargo.lock;
    cargoBuildFlags = [
      "-p"
      "uniffi-bindgen-react-native"
      "--bin"
      "uniffi-bindgen-react-native"
    ];
    doCheck = false;
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
      cp ${nodeNative}/*.node "$package/"
    '';
  };
in
{
  inherit
    rev
    src
    cli
    core
    node
    wasm
    ;
}
