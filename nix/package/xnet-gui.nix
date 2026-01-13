# Using a manual derivation since the gui requires some custom options
# that is easier to do in a manual crane build
{ xnetFileset
, lib
, pkg-config
, perl
, openssl
, sqlite
, sqlcipher
, zlib
, zstd
, fontconfig
, freetype
, curl
, stdenv
, libxkbcommon
, wayland
, xorg
, libGL
, libX11
, libXext
, vulkan-loader
, apple-sdk_15
, mkShell
, xmtp
}:
let
  src = ./../..;
  commonArgs = {
    description = "xNet GUI";
    src = xnetFileset (src + /apps/xnet/gui);
    pname = "xnet-gui";
    inherit (xmtp.craneLib.crateNameFromCargoToml { cargoToml = src + /Cargo.toml; }) version;
    strictDeps = true;
    doCheck = false;
    # LD_LIBRARY_PATH needed for buildscripts
    LD_LIBRARY_PATH = lib.makeLibraryPath [ openssl zstd zlib ];
    nativeBuildInputs = [
      pkg-config
      perl
    ];
    buildInputs = [
      openssl
      sqlite
      sqlcipher
      zlib
      zstd

      fontconfig
      freetype
      curl
    ] ++ lib.optionals stdenv.hostPlatform.isLinux [
      libxkbcommon
      wayland
      xorg.libxcb
      libGL
      libX11
      libXext
      vulkan-loader
    ] ++ lib.optionals stdenv.hostPlatform.isDarwin [ apple-sdk_15 ];
    # Add runtime_shaders feature on Darwin to avoid needing Xcode's metal tools
    # See: https://github.com/zed-industries/zed/discussions/7016
    cargoExtraArgs = "-p xnet-gui" + lib.optionalString stdenv.hostPlatform.isDarwin " --features gpui/runtime_shaders";
  } // lib.optionalAttrs stdenv.hostPlatform.isLinux {
    NIX_LDFLAGS = "-rpath ${lib.makeLibraryPath [ vulkan-loader wayland ]}";
    dontPatchELF = true;
  };

  cargoArtifacts = xmtp.craneLib.buildDepsOnly commonArgs;
  bin = xmtp.craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; });
  devShell = mkShell {
    inputsFrom = [ bin ];
    LD_LIBRARY_PATH = "${lib.makeLibraryPath commonArgs.buildInputs}:/run/opengl-driver/lib";
  };
in
{
  inherit bin devShell;
}
