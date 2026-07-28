# Pre-built SwiftFormat binary from GitHub releases.
# This avoids building the Swift compiler from source in Nix — swift is
# broken/uncached on Linux in current nixpkgs (clang 16 rejects the stdenv's
# -mtls-dialect=gnu2). The Linux release binaries are fully static, so no
# patching is needed; the darwin zip ships a universal binary.
{
  stdenv,
  fetchurl,
  unzip,
}:

let
  version = "0.62.1";

  # When updating: change version and refresh every hash together.
  # Get a hash with: nix store prefetch-file <url>
  sources = {
    x86_64-linux = {
      asset = "swiftformat_linux.zip";
      binary = "swiftformat_linux";
      hash = "sha256-Yf9V81geIUSkrRFIMRZxAsOL6FPfdcFHfSC0Co6BIKo=";
    };
    aarch64-linux = {
      asset = "swiftformat_linux_aarch64.zip";
      binary = "swiftformat_linux_aarch64";
      hash = "sha256-N0sZdQgpcKwXybMHY528ChSGVkNM1hLP8GlX4nkCDzQ=";
    };
    x86_64-darwin = {
      asset = "swiftformat.zip";
      binary = "swiftformat";
      hash = "sha256-fLHLH64EkyBHxwFUQcVDhI6OYOFXLYCNCA4KHxZhEUo=";
    };
    aarch64-darwin = {
      asset = "swiftformat.zip";
      binary = "swiftformat";
      hash = "sha256-fLHLH64EkyBHxwFUQcVDhI6OYOFXLYCNCA4KHxZhEUo=";
    };
  };

  source =
    sources.${stdenv.hostPlatform.system}
      or (throw "swiftformat: unsupported system ${stdenv.hostPlatform.system}");
in
stdenv.mkDerivation {
  pname = "swiftformat";
  inherit version;

  src = fetchurl {
    url = "https://github.com/nicklockwood/SwiftFormat/releases/download/${version}/${source.asset}";
    inherit (source) hash;
  };

  nativeBuildInputs = [ unzip ];
  sourceRoot = ".";

  unpackPhase = ''
    unzip $src
  '';

  installPhase = ''
    install -Dm755 ${source.binary} $out/bin/swiftformat
  '';

  meta = {
    description = "Code formatting tool for Swift (pre-built binary)";
    homepage = "https://github.com/nicklockwood/SwiftFormat";
    mainProgram = "swiftformat";
    platforms = builtins.attrNames sources;
  };
}
