#!/usr/bin/env bash
set -euo pipefail

adb=$1
serial=$2

device() {
  timeout 15 "$adb" -s "$serial" "$@"
}

# The backend uses the host clock. A slow guest clock delays message expiry.
device root
device wait-for-device
# Settings can start after adbd accepts commands. API 23 uses a content provider.
for attempt in {1..30}; do
  if auto_time=$(device shell settings get global auto_time | tr -d '\r') &&
    [[ "$auto_time" =~ ^[01]$ ]]; then
    break
  fi
  if ((attempt == 30)); then
    echo "Android settings service did not become ready ($serial)" >&2
    exit 1
  fi
  sleep 1
done
device shell settings put global auto_time 0

# Retry a small transport delay. Fail before tests if the clock stays wrong.
for attempt in 1 2 3; do
  host_time=$(date -u +%s)
  device shell date -u "@$host_time"
  guest_time=$(device shell date -u +%s | tr -d '\r')
  if [[ ! "$guest_time" =~ ^[0-9]+$ ]]; then
    echo "Cannot read the emulator clock ($serial): $guest_time" >&2
    exit 1
  fi
  host_time=$(date -u +%s)
  offset=$((guest_time - host_time))
  echo "Emulator clock ($serial), attempt $attempt: offset $offset seconds" >&2
  if ((offset >= -2 && offset <= 2)); then
    exit 0
  fi
done

echo "Emulator clock differs from the host by more than 2 seconds ($serial)" >&2
exit 1
