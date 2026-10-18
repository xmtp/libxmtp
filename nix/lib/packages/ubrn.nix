# The `@ubjs` runtimes of the pinned uniffi-bindgen-react-native fork.
# `src` is the `ubrn-src` flake input, so evaluation reads the fork's
# `Cargo.lock` without a build (no IFD).
{
  lib,
  stdenv,
  buildNpmPackage,
  callPackage,
  runCommand,
  python311,
  xmtp,
  src,
}:
let
  forkUrl = "git+https://github.com/neekolas/uniffi-bindgen-react-native.git";
  forkPackages = [
    "ubrn_bindgen"
    "ubrn_common"
    "uniffi-runtime-wasm"
  ];
  expectedRev =
    (builtins.fromTOML (builtins.readFile ../../../Cargo.toml)).workspace.metadata."xmtp-sdk-fork".rev;
  lockedPackages = builtins.filter (
    package: builtins.elem package.name forkPackages
  ) (builtins.fromTOML (builtins.readFile ../../../Cargo.lock)).package;
  missing = lib.subtractLists (map (package: package.name) lockedPackages) forkPackages;
  # Cargo.lock must take each fork package from the fork at the workspace pin.
  unpinned = map (package: "${package.name} (${package.source or "no source"})") (
    builtins.filter (
      package:
      !(lib.hasPrefix forkUrl (package.source or ""))
      || lib.last (lib.splitString "#" package.source) != expectedRev
    ) lockedPackages
  );
  # The one check of the fork pin; it fails evaluation before any build.
  rev =
    assert lib.assertMsg (src.rev == expectedRev)
      "SDK fork pin: the ubrn-src flake input is at ${src.rev}, but Cargo.toml [workspace.metadata.xmtp-sdk-fork] rev is ${expectedRev}";
    assert lib.assertMsg (
      missing == [ ]
    ) "SDK fork pin: Cargo.lock has no ${lib.concatStringsSep ", " missing} package";
    assert lib.assertMsg (unpinned == [ ])
      "SDK fork pin: Cargo.lock does not take ${lib.concatStringsSep ", " unpinned} from ${forkUrl} at ${expectedRev}";
    expectedRev;
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
  # RN uses the fork's core TypeScript package and JSI C++ headers. There is
  # no separately published RN runtime in the fork. Keep the exact shipped
  # assets in one product; the stager checks this manifest before vendoring.
  reactNative =
    runCommand "ubjs-react-native-${rev}"
      {
        nativeBuildInputs = [ python311 ];
        passthru = {
          runtimeRevision = rev;
        };
      }
      ''
        mkdir -p "$out/core" "$out/cpp/includes"
        cp -RL ${core}/lib/node_modules/@ubjs/core/. "$out/core/"
        cp -R ${src}/cpp/includes/. "$out/cpp/includes/"
        cp ${src}/LICENSE "$out/LICENSE"
        python3 - "$out" ${lib.escapeShellArg rev} <<'PYTHON'
        import hashlib, json, pathlib, sys
        root = pathlib.Path(sys.argv[1])
        assets = {
            file.relative_to(root).as_posix(): hashlib.sha256(file.read_bytes()).hexdigest()
            for file in sorted(root.rglob("*")) if file.is_file()
        }
        (root / "runtime-provenance.json").write_text(json.dumps({
            "schema": 1, "forkRevision": sys.argv[2], "assets": assets,
        }, sort_keys=True) + "\n")
        PYTHON
      '';
  # The host addon is the Crane build that the host Node SDK pair also uses.
  nodeAddon = callPackage ../../package/xmtp-sdk-node-runtime.nix {
    ubrnSrc = src;
    runtimeRevision = rev;
  };
  addon = "uniffi-runtime-napi.${xmtp.toNapiTarget stdenv.hostPlatform.rust.rustcTarget}.node";
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
      cp ${nodeAddon}/${addon} "$package/"
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
    reactNative
    ;
}
