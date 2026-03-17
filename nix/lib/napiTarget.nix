# rustToNapiTarget :: string -> string
# Converts a Rust target triple (e.g. "aarch64-unknown-linux-musl")
# to a napi-rs target string (e.g. "linux-arm64-musl").
rustTarget:
let
  parts = builtins.filter builtins.isString (builtins.split "-" rustTarget);
  len = builtins.length parts;

  arch = builtins.elemAt parts 0;
  # parts[1] is vendor ("unknown", "apple") — skip
  os = builtins.elemAt parts 2;
  abi = if len >= 4 then builtins.elemAt parts 3 else null;

  archMap = {
    "x86_64" = "x64";
    "i686" = "ia32";
    "aarch64" = "arm64";
    "armv7l" = "arm";
  };

  osMap = {
    "linux" = "linux";
    "darwin" = "darwin";
    "windows" = "win32";
  };

  napiArch = archMap.${arch} or (throw "rustToNapiTarget: unknown arch '${arch}'");
  napiOs = osMap.${os} or (throw "rustToNapiTarget: unknown OS '${os}'");

  base = "${napiOs}-${napiArch}";
in
if abi == null then base else "${base}-${abi}"
