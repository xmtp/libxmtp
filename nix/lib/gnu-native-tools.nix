{
  writeShellScript,
  writeText,
  zig,
  linkerEnv,
}:
let
  # Reuse the existing compiler argument/path contracts without changing the
  # Apple wrappers or the compiler selected by cargo-zigbuild.
  remapBody = builtins.readFile ./apple-native-tool.sh;
  remap = writeText "xmtp-native-remap.sh" remapBody;
  cc = writeShellScript "xmtp-gnu-zig-cc" ''
    exec ${zig}/bin/zig cc "$@"
  '';
  cxx = writeShellScript "xmtp-gnu-zig-cxx" ''
    exec ${zig}/bin/zig c++ "$@"
  '';
in
{
  stableCc = writeShellScript "xmtp-gnu-cc" ''
    export XMTP_NATIVE_LINKER_ENV=${linkerEnv}
    ${builtins.readFile ./gnu-native-cc.sh}
  '';
  rustc = writeShellScript "xmtp-gnu-rustc" ''
    set -- rustc "$@"
    ${remapBody}
  '';
  zig = writeShellScript "xmtp-gnu-zig" ''
    export XMTP_NATIVE_ZIG=${zig}/bin/zig
    export XMTP_NATIVE_CC=${cc}
    export XMTP_NATIVE_CXX=${cxx}
    export XMTP_NATIVE_REMAP_TOOL=${remap}
    ${builtins.readFile ./gnu-native-zig.sh}
  '';
}
