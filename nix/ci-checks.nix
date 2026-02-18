# CI checks to ensure all packages and dev shells build
_: {
  perSystem =
    { self', pkgs, ... }:
    {
      checks.all-packages = pkgs.linkFarm "all-packages" {
        inherit (self'.packages)
          android-libs-fast
          wasm-bindings
          wasm-bindgen-cli
          mls_validation_service
          ;
      };
      checks.dev-shells = pkgs.linkFarm "dev-shells" self'.devShells;
    };
}
