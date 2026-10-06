#!/usr/bin/env bash
set -euo pipefail
: "${VERSION:?Expected the CocoaPods release version}"

# CocoaPods uses the first three MD5 digits of the pod name. XMTP starts with ab7.
spec_is_published() {
  local url
  url="https://raw.githubusercontent.com/CocoaPods/Specs/master/Specs/a/b/7/XMTP/${VERSION}/XMTP.podspec.json"
  [ "$(curl -sS -o /dev/null -w '%{http_code}' "$url")" = "200" ]
}

if spec_is_published; then
  echo "Version $VERSION already published to CocoaPods, skipping"
  exit 0
fi

log=$(mktemp)
trap 'rm -f "$log"' EXIT
for attempt in 1 2 3; do
  if pod trunk push XMTP.podspec --allow-warnings --skip-tests 2>&1 | tee "$log"; then
    exit 0
  fi

  # A failed response can follow a successful commit. Allow the CDN to catch up.
  for poll in 0 1 2 3 4 5 6; do
    if [ "$poll" -gt 0 ]; then sleep 20; fi
    if spec_is_published; then
      echo "Version $VERSION is published; treating the push failure as transient"
      exit 0
    fi
  done

  if [ "$attempt" -eq 3 ] || ! grep -Fq 'Calling the GitHub commit API timed out.' "$log"; then
    echo "Version $VERSION is not published to CocoaPods"
    exit 1
  fi
  echo "CocoaPods GitHub commit API timed out; retrying push ($attempt/3)"
done
