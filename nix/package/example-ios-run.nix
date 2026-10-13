{
  writeShellApplication,
  bash,
  jq,
  git,
  stdenv,
  xmtp,
}:
writeShellApplication {
  name = "example-ios";
  # iosEnv intentionally clears DEVELOPER_DIR for xcode-select.
  excludeShellChecks = [ "SC1007" ];
  runtimeInputs = [
    bash
    jq
    git
  ];
  text = ''
    root="$(git rev-parse --show-toplevel 2>/dev/null)" || { echo 'Run this from a libxmtp checkout.' >&2; exit 1; }
    if [[ ! -f "$root/apps/example-ios/dev/start" || ! -f "$root/sdks/ios/dev/bindings" ]]; then
      echo 'Run this from a libxmtp checkout.' >&2
      exit 1
    fi
    ${xmtp.iosEnv.envSetup stdenv.hostPlatform.rust.rustcTarget}
    unset SDKROOT
    exec bash "$root/apps/example-ios/dev/start" "$@"
  '';
}
