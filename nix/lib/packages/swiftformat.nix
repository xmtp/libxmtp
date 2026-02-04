# Pre-built SwiftFormat binary from GitHub releases
# This avoids building the Swift compiler from source in Nix
{ stdenv, fetchurl, unzip }:

stdenv.mkDerivation rec {
  pname = "swiftformat";
  version = "0.58.5";

  # When updating: change version, url, and hash together
  # Get the hash with: nix-prefetch-url --type sha256 --unpack <url>
  src = fetchurl {
    url = "https://github.com/nicklockwood/SwiftFormat/releases/download/0.58.5/swiftformat.zip";
    hash = "sha256-49Weu7qzVn+fRg4tJJg+1E3jUb2AuetbPLFNBjF98Fw=";
  };

  nativeBuildInputs = [ unzip ];
  sourceRoot = ".";

  unpackPhase = ''
    unzip $src
  '';

  installPhase = ''
    mkdir -p $out/bin
    cp swiftformat $out/bin/
    chmod +x $out/bin/swiftformat
  '';

  meta = {
    description = "Code formatting tool for Swift (pre-built binary)";
    homepage = "https://github.com/nicklockwood/SwiftFormat";
  };
}
