# List of build targets for iOS & their equivalent rust/cargo targets
# List targets with `nix repl -f '<nixpkgs>' -I nixpkgs=channel:nixos-24.05`
#   then, `pkgsCross.<TAB>`
# https://nix.dev/tutorials/cross-compilation
{
  "iphone64-simulator" = {
    crossSystemConfig = "x86_64-apple-ios";
    rustTarget = "x86_64-apple-ios";
  };

  "iphone64-simulator-arm" = {
    crossSystemConfig = "aarch64-apple-ios";
    rustTarget = "x86_64-apple-ios-sim";
  };

  "iphone64" = {
    crossSystemConfig = "aarch64-apple-ios";
    rustTarget = "aarch64-apple-ios";
  };

  "aarch64-darwin" = {
    crossSystemConfig = "aarch64-apple-darwin";
    rustTarget = "aarch64-apple-darwin";
  };

  "x86_64-apple-darwin" = {
    crossSystemConfig = "x86_64-apple-darwin";
    rustTarget = "x86_64-apple-darwin";
  };
}



#
#
#
#
# aarch64-darwin = {
#   default = normal
#   cross-iphone64 = cross derivation for iphone64
# }
#
#
#

