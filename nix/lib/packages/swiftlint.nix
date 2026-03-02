# Pre-built SwiftLint binary from GitHub releases
# This avoids building the Swift compiler from source in Nix
{
  stdenv,
  fetchurl,
  unzip,
}:

stdenv.mkDerivation rec {
  pname = "swiftlint";
  version = "0.62.1";

  # When updating: change version, url, and hash together
  # Get the hash with: nix-prefetch-url --type sha256 --unpack <url>
  src = fetchurl {
    url = "https://github.com/realm/SwiftLint/releases/download/0.62.1/portable_swiftlint.zip";
    hash = "sha256-VB20vZT4z4+6q3YvWX5/DkkBan+MpccNhrQ3CnzSNkE=";
  };

  nativeBuildInputs = [ unzip ];
  sourceRoot = ".";

  unpackPhase = ''
    unzip $src
  '';

  installPhase = ''
    mkdir -p $out/bin
    cp swiftlint $out/bin/
    chmod +x $out/bin/swiftlint
  '';

  meta = {
    description = "Swift linter tool (pre-built binary)";
    homepage = "https://github.com/realm/SwiftLint";
  };
}
