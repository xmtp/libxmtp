{ self, inputs, ... }:
{
  perSystem =
    {
      pkgs,
      system,
      lib,
      ...
    }:
    let
      # JNA platform directory names
      jnaPlatform =
        if pkgs.stdenv.isDarwin && pkgs.stdenv.isAarch64 then
          "darwin-aarch64"
        else if pkgs.stdenv.isDarwin && pkgs.stdenv.isx86_64 then
          "darwin-x86-64"
        else if pkgs.stdenv.isLinux && pkgs.stdenv.isx86_64 then
          "linux-x86-64"
        else if pkgs.stdenv.isLinux && pkgs.stdenv.isAarch64 then
          "linux-aarch64"
        else
          throw "Unsupported platform for JVM native libs";

      ext = if pkgs.stdenv.isDarwin then "dylib" else "so";

      mkJvmNative = p: p.callPackage ./package/jvm-native.nix;
      dylib = (mkJvmNative pkgs { }).dylib;

      # Single-platform output (current host)
      jvm-native-fast = pkgs.linkFarm "xmtpv3-jvm-native-fast" [
        {
          name = "${jnaPlatform}/libuniffi_xmtpv3.${ext}";
          path = "${dylib}/libuniffi_xmtpv3.${ext}";
        }
      ];

      # For a full multi-platform build, cross-compile from a CI matrix
      # and merge the outputs into a single directory.
    in
    {
      packages = {
        inherit jvm-native-fast;
      };
    };
}
