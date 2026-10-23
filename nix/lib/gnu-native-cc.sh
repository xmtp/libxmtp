# shellcheck shell=bash
# OpenSSL records CC verbatim. Keep its public entry point in the store while
# executing cargo-zigbuild's target linker with its original flags and filters.
set -euo pipefail
linker_env=${XMTP_NATIVE_LINKER_ENV:?GNU target linker variable is not set}
compiler=${!linker_env}
exec "${compiler:?cargo-zigbuild must set its GNU target linker}" "$@"
