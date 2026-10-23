# shellcheck shell=bash
# cargo-zigbuild still selects the target and linker flags. Remap only the
# compiler frontends; version, env, archive, and other Zig commands pass through.
set -euo pipefail
case "${1-}" in
  cc)
    shift
    set -- cc "$@"
    ;;
  c++)
    shift
    set -- cxx "$@"
    ;;
  *)
    exec "${XMTP_NATIVE_ZIG:?native Zig is not set}" "$@"
    ;;
esac
# The Nix wrapper supplies the store copy of this shared implementation.
# shellcheck source=nix/lib/apple-native-tool.sh
source "${XMTP_NATIVE_REMAP_TOOL:?compiler remapping tool is not set}" "$@"
