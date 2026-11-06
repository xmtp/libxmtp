{
  lib,
  runCommand,
  packages,
  securePackages,
}:
let
  usesNode =
    package: node:
    let
      selected = builtins.filter (
        input:
        builtins.elem (input.pname or null) [
          "nodejs"
          "nodejs-slim"
        ]
      ) ((package.nativeBuildInputs or [ ]) ++ (package.buildInputs or [ ]));
    in
    selected != [ ] && builtins.all (input: input.drvPath == node.drvPath) selected;
  consumers = {
    napi = packages.napi-rs-cli;
    markdownlint = packages.markdownlint-cli;
    ubjs-core = packages.xmtp.ubrn.core;
    ubjs-node = packages.xmtp.ubrn.node;
    ubjs-wasm = packages.xmtp.ubrn.wasm;
  };
  checks = lib.mapAttrsToList (
    name: package:
    lib.assertMsg (usesNode package securePackages.nodejs_24) "${name} must build and run with the scoped Node 24 security release"
  ) consumers;
in
assert builtins.all (check: check) checks;
assert lib.assertMsg (usesNode packages.xmtp-pnpm securePackages.nodejs-slim_26)
  "pnpm must build and run with the scoped Node 26 security release";
assert lib.assertMsg (
  packages.playwright.drvPath == securePackages.playwright.drvPath
) "Playwright must use the scoped Node runtime and build hooks";
assert lib.assertMsg (
  packages.playwright-driver.drvPath == securePackages.playwright-driver.drvPath
) "Playwright's driver must use the scoped Node runtime and build hooks";
assert lib.assertMsg (
  packages.nodejs.drvPath != securePackages.nodejs.drvPath
) "Node security overrides must not invalidate unrelated upstream package builds";
runCommand "node-runtime-consumers" { } ''
  touch "$out"
''
