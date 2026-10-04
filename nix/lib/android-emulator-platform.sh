#!/usr/bin/env bash
# Source this before creating an AVD. The API 23 CI caller needs Linux x86_64.
ANDROID_EMULATOR_API="${NIX_ANDROID_EMULATOR_API:-$ANDROID_DEFAULT_EMULATOR_API}"
case "$ANDROID_EMULATOR_API" in
  "$ANDROID_DEFAULT_EMULATOR_API") ;;
  23)
    if [[ "$ANDROID_API23_SUPPORTED" != 1 ]]; then
      echo "API 23 emulator requires a Linux x86_64 host" >&2
      return 1
    fi
    ;;
  *)
    echo "Unsupported Android emulator API: $ANDROID_EMULATOR_API" >&2
    return 1
    ;;
esac
