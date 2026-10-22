final: prev:
let
  runtimeDependencies = {
    brotli = prev.brotli.overrideAttrs (previousAttrs: {
      patches = previousAttrs.patches ++ [
        (prev.fetchurl {
          url = "https://github.com/google/brotli/commit/0ed4268a4401abcb4472096340477cf4af7d1a2b.patch";
          hash = "sha256-gzyh1x7thb3jwhxjpu4xATh3Qbiqi5qCJRNAlKZ3thk=";
        })
      ];
    });
    c-ares = prev.c-ares.overrideAttrs (_: {
      version = "1.34.9";
      src = prev.fetchurl {
        url = "https://github.com/c-ares/c-ares/releases/download/v1.34.9/c-ares-1.34.9.tar.gz";
        hash = "sha256-k2S3ChLrWaBoMPtIjZNX+QEA3/DZiyPqNZOhg9Q+by8=";
      };
    });
    openssl = prev.openssl.overrideAttrs (_: {
      version = "3.6.5";
      src = prev.fetchurl {
        url = "https://github.com/openssl/openssl/releases/download/openssl-3.6.5/openssl-3.6.5.tar.gz";
        hash = "sha256-ohV8KDDv3sN4iTmwDJsGODBtPwu7dtxIMu5QO7OX35g=";
      };
    });
    simdutf = prev.simdutf.overrideAttrs (previousAttrs: {
      cmakeFlags = previousAttrs.cmakeFlags ++ [ "-DSIMDUTF_CXX_STANDARD=20" ];
    });
    ngtcp2 = prev.ngtcp2.override {
      inherit (runtimeDependencies) brotli openssl;
    };
    nghttp2 = prev.nghttp2.override {
      inherit (runtimeDependencies) openssl;
      c-aresMinimal = runtimeDependencies.c-ares;
    };
  };
  updateNode =
    package: version: hash:
    (package.override {
      inherit (runtimeDependencies) openssl;
      callPackage = prev.newScope runtimeDependencies;
    }).overrideAttrs
      (
        finalAttrs: previousAttrs: {
          inherit version;
          src = prev.fetchurl {
            url = "https://nodejs.org/dist/v${version}/node-v${version}.tar.xz";
            inherit hash;
          };
          patches = builtins.filter (
            patch:
            !(builtins.elem (builtins.baseNameOf patch) [
              "fix-temporal-integration-with-shared-icu.patch"
              "configure-emulator.patch"
            ])
          ) previousAttrs.patches;
          passthru = previousAttrs.passthru // {
            tests = previousAttrs.passthru.tests // {
              dependencies = prev.callPackage ./check-node-dependencies.nix {
                node = finalAttrs.finalPackage;
                dependencies = runtimeDependencies;
                unpatchedDependencies = prev;
              };
              version = prev.testers.testVersion {
                package = finalAttrs.finalPackage;
                version = "v${version}";
              };
            };
          };
          meta = previousAttrs.meta // {
            changelog = "https://github.com/nodejs/node/releases/tag/v${version}";
          };
        }
      );
in
{
  nodejs-slim_26 =
    updateNode prev.nodejs-slim_26 "26.12.1"
      "sha256-BT47KWhZttjZP5nvl/qc7BxO4XHHKWgQDGSZeMHb04w=";
  nodejs-slim_24 =
    updateNode prev.nodejs-slim_24 "24.22.1"
      "sha256-T1N5eWKAza6wwvNY/QS2oWzE+336g7WoyRqdljtDUjE=";
}
